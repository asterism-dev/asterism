use std::path::Path;

use asterism_proto::types::{Project, Session, SessionKind, SessionStatus, Task};
use rusqlite::types::Type;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::de::DeserializeOwned;
use serde::Serialize;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS projects (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    path TEXT NOT NULL UNIQUE
);
CREATE TABLE IF NOT EXISTS tasks (
    id INTEGER PRIMARY KEY,
    project_id INTEGER NOT NULL,
    title TEXT NOT NULL,
    slug TEXT NOT NULL DEFAULT '',
    branch TEXT NOT NULL DEFAULT '',
    base_branch TEXT NOT NULL,
    worktree_path TEXT NOT NULL DEFAULT '',
    prompt TEXT,
    archived INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS sessions (
    id INTEGER PRIMARY KEY,
    task_id INTEGER NOT NULL,
    kind TEXT NOT NULL,
    status TEXT NOT NULL,
    agent_ref TEXT
);
";

const TASK_COLUMNS: &str =
    "id, project_id, title, slug, branch, base_branch, worktree_path, prompt, archived";
const SESSION_COLUMNS: &str = "id, task_id, kind, status, agent_ref";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSession {
    pub session: Session,
    pub agent_ref: Option<String>,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> rusqlite::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> rusqlite::Result<Self> {
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    pub fn add_project(&self, name: &str, path: &str) -> rusqlite::Result<Project> {
        self.conn.execute(
            "INSERT OR IGNORE INTO projects (name, path) VALUES (?1, ?2)",
            params![name, path],
        )?;
        self.conn.query_row("SELECT id, name, path FROM projects WHERE path = ?1", [path], project_row)
    }

    pub fn projects(&self) -> rusqlite::Result<Vec<Project>> {
        let mut stmt = self.conn.prepare("SELECT id, name, path FROM projects ORDER BY id")?;
        let rows = stmt.query_map([], project_row)?;
        rows.collect()
    }

    pub fn project(&self, id: i64) -> rusqlite::Result<Option<Project>> {
        self.conn
            .query_row("SELECT id, name, path FROM projects WHERE id = ?1", [id], project_row)
            .optional()
    }

    pub fn remove_project(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "DELETE FROM sessions WHERE task_id IN (SELECT id FROM tasks WHERE project_id = ?1)",
            [id],
        )?;
        self.conn.execute("DELETE FROM tasks WHERE project_id = ?1", [id])?;
        self.conn.execute("DELETE FROM projects WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn insert_task(
        &self,
        project_id: i64,
        title: &str,
        prompt: Option<&str>,
        base_branch: &str,
    ) -> rusqlite::Result<i64> {
        self.conn.execute(
            "INSERT INTO tasks (project_id, title, prompt, base_branch) VALUES (?1, ?2, ?3, ?4)",
            params![project_id, title, prompt, base_branch],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn set_task_location(
        &self,
        id: i64,
        slug: &str,
        branch: &str,
        worktree_path: &str,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE tasks SET slug = ?2, branch = ?3, worktree_path = ?4 WHERE id = ?1",
            params![id, slug, branch, worktree_path],
        )?;
        Ok(())
    }

    pub fn delete_task(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM tasks WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn set_task_archived(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute("UPDATE tasks SET archived = 1 WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn task(&self, id: i64) -> rusqlite::Result<Option<Task>> {
        self.conn
            .query_row(&format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id = ?1"), [id], task_row)
            .optional()
    }

    pub fn tasks(&self, project_id: Option<i64>, include_archived: bool) -> rusqlite::Result<Vec<Task>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {TASK_COLUMNS} FROM tasks
             WHERE (?1 IS NULL OR project_id = ?1) AND (?2 OR archived = 0) ORDER BY id"
        ))?;
        let rows = stmt.query_map(params![project_id, include_archived], task_row)?;
        rows.collect()
    }

    pub fn insert_session(
        &self,
        task_id: i64,
        kind: &SessionKind,
        status: SessionStatus,
    ) -> rusqlite::Result<i64> {
        self.conn.execute(
            "INSERT INTO sessions (task_id, kind, status) VALUES (?1, ?2, ?3)",
            params![task_id, to_text(kind), to_text(&status)],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn set_session_status(&self, id: i64, status: SessionStatus) -> rusqlite::Result<()> {
        self.conn
            .execute("UPDATE sessions SET status = ?2 WHERE id = ?1", params![id, to_text(&status)])?;
        Ok(())
    }

    pub fn set_session_agent_ref(&self, id: i64, agent_ref: &str) -> rusqlite::Result<()> {
        self.conn
            .execute("UPDATE sessions SET agent_ref = ?2 WHERE id = ?1", params![id, agent_ref])?;
        Ok(())
    }

    pub fn session(&self, id: i64) -> rusqlite::Result<Option<StoredSession>> {
        self.conn
            .query_row(&format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1"), [id], session_row)
            .optional()
    }

    pub fn sessions(&self, task_id: Option<i64>) -> rusqlite::Result<Vec<StoredSession>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {SESSION_COLUMNS} FROM sessions WHERE (?1 IS NULL OR task_id = ?1) ORDER BY id"
        ))?;
        let rows = stmt.query_map([task_id], session_row)?;
        rows.collect()
    }
}

fn to_text<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

fn from_text<T: DeserializeOwned>(column: usize, text: String) -> rusqlite::Result<T> {
    serde_json::from_str(&text)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(column, Type::Text, Box::new(e)))
}

fn project_row(row: &Row) -> rusqlite::Result<Project> {
    Ok(Project { id: row.get(0)?, name: row.get(1)?, path: row.get(2)? })
}

fn task_row(row: &Row) -> rusqlite::Result<Task> {
    Ok(Task {
        id: row.get(0)?,
        project_id: row.get(1)?,
        title: row.get(2)?,
        slug: row.get(3)?,
        branch: row.get(4)?,
        base_branch: row.get(5)?,
        worktree_path: row.get(6)?,
        prompt: row.get(7)?,
        archived: row.get::<_, i64>(8)? != 0,
    })
}

fn session_row(row: &Row) -> rusqlite::Result<StoredSession> {
    Ok(StoredSession {
        session: Session {
            id: row.get(0)?,
            task_id: row.get(1)?,
            kind: from_text(2, row.get(2)?)?,
            status: from_text(3, row.get(3)?)?,
        },
        agent_ref: row.get(4)?,
    })
}

#[cfg(test)]
mod tests {
    use asterism_proto::types::{SessionKind, SessionStatus};
    use super::*;

    #[test]
    fn add_project_is_idempotent_by_path() {
        let store = Store::open_in_memory().unwrap();
        let a = store.add_project("repo", "/src/repo").unwrap();
        let b = store.add_project("repo", "/src/repo").unwrap();
        assert_eq!(a, b);
        assert_eq!(store.projects().unwrap(), vec![a]);
    }

    #[test]
    fn tasks_roundtrip_and_filter_archived() {
        let store = Store::open_in_memory().unwrap();
        let p = store.add_project("repo", "/src/repo").unwrap();
        let id = store.insert_task(p.id, "Fix login", Some("go"), "main").unwrap();
        store.set_task_location(id, "1-fix-login", "asterism/1-fix-login", "/wt").unwrap();
        let task = store.task(id).unwrap().unwrap();
        assert_eq!(task.branch, "asterism/1-fix-login");
        assert_eq!(task.prompt.as_deref(), Some("go"));
        assert!(!task.archived);

        store.set_task_archived(id).unwrap();
        assert!(store.tasks(Some(p.id), false).unwrap().is_empty());
        assert_eq!(store.tasks(None, true).unwrap().len(), 1);
    }

    #[test]
    fn sessions_store_kind_status_and_agent_ref() {
        let store = Store::open_in_memory().unwrap();
        let p = store.add_project("repo", "/src/repo").unwrap();
        let task = store.insert_task(p.id, "t", None, "main").unwrap();
        let kind = SessionKind::Agent { name: "claude".into() };
        let id = store.insert_session(task, &kind, SessionStatus::Working).unwrap();
        store.set_session_status(id, SessionStatus::WaitingInput).unwrap();
        store.set_session_agent_ref(id, "abc").unwrap();

        let stored = store.session(id).unwrap().unwrap();
        assert_eq!(stored.session.kind, kind);
        assert_eq!(stored.session.status, SessionStatus::WaitingInput);
        assert_eq!(stored.agent_ref.as_deref(), Some("abc"));
        assert_eq!(store.sessions(Some(task)).unwrap().len(), 1);
        assert!(store.sessions(Some(task + 1)).unwrap().is_empty());
    }

    #[test]
    fn remove_project_drops_its_tasks_and_sessions() {
        let store = Store::open_in_memory().unwrap();
        let p = store.add_project("repo", "/src/repo").unwrap();
        let task = store.insert_task(p.id, "t", None, "main").unwrap();
        store.insert_session(task, &SessionKind::Shell, SessionStatus::Exited).unwrap();
        store.remove_project(p.id).unwrap();
        assert!(store.projects().unwrap().is_empty());
        assert!(store.tasks(None, true).unwrap().is_empty());
        assert!(store.sessions(None).unwrap().is_empty());
    }
}
