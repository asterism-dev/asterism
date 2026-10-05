use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::*;
use asterism_proto::PROTO_VERSION;
use tokio::sync::{broadcast, watch, Notify};

use crate::agent_settings;
use crate::agents::{self, AgentProfile};
use crate::config::{self, Config};
use crate::error::{Error, Result};
use crate::paths::Paths;
use crate::session::{Pty, Snapshot, SpawnSpec, SCROLLBACK_LINES};
use crate::store::Store;
use crate::node_settings::LOCAL_OWNER;
use crate::{git, github, lock, node_settings, repo_source, status};

const DEFAULT_ROWS: u16 = 40;
const DEFAULT_COLS: u16 = 120;
const REMOVE_GRACE: Duration = Duration::from_secs(2);
// ponytail: fixed pause so TUIs treat Enter as a submit, not part of the paste; make it per-profile if an agent needs more.
const SUBMIT_DELAY: Duration = Duration::from_millis(100);
const SLUG_MAX: usize = 40;

struct LiveSession {
    pty: Arc<Pty>,
    status: Arc<watch::Sender<SessionStatus>>,
    hooks_active: Arc<AtomicBool>,
}

/// Overridable external tools; tests inject a fake `gh` and an isolated git environment.
pub struct DaemonOptions {
    pub gh_bin: PathBuf,
    pub git_env: Vec<(String, String)>,
}

impl Default for DaemonOptions {
    fn default() -> Self {
        Self { gh_bin: PathBuf::from("gh"), git_env: Vec::new() }
    }
}

pub struct Daemon {
    options: DaemonOptions,
    paths: Paths,
    store: Mutex<Store>,
    live: Mutex<HashMap<i64, Arc<LiveSession>>>,
    events: broadcast::Sender<Event>,
    shutdown: Notify,
}

impl Daemon {
    pub fn new(paths: Paths) -> Result<Arc<Self>> {
        Self::with_options(paths, DaemonOptions::default())
    }

    pub fn with_options(paths: Paths, options: DaemonOptions) -> Result<Arc<Self>> {
        paths.ensure_dirs()?;
        agent_settings::write_claude_settings(&paths)?;
        let store = Store::open(&paths.db())?;
        let (events, _) = broadcast::channel(1024);
        Ok(Arc::new(Self {
            options,
            paths,
            store: Mutex::new(store),
            live: Mutex::new(HashMap::new()),
            events,
            shutdown: Notify::new(),
        }))
    }

    pub fn paths(&self) -> &Paths {
        &self.paths
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    pub fn request_shutdown(&self) {
        self.shutdown.notify_one();
    }

    pub async fn shutdown_requested(&self) {
        self.shutdown.notified().await;
    }

    fn store(&self) -> MutexGuard<'_, Store> {
        lock(&self.store)
    }

    fn emit(&self, event: Event) {
        let _ = self.events.send(event);
    }

    pub fn hello(&self, params: HelloParams) -> Result<HelloResult> {
        if params.proto_version != PROTO_VERSION {
            return Err(Error::new(
                ErrorKind::IncompatibleVersion,
                format!("daemon speaks protocol {PROTO_VERSION}, client speaks {}", params.proto_version),
            ));
        }
        Ok(HelloResult {
            proto_version: PROTO_VERSION,
            daemon_version: env!("CARGO_PKG_VERSION").into(),
            daemon_build: asterism_proto::BUILD_ID.into(),
            pid: std::process::id(),
            hostname: gethostname::gethostname().to_string_lossy().into_owned(),
            os: std::env::consts::OS.into(),
            agents: agents::PROFILES
                .iter()
                .map(|p| AgentInfo { name: p.name.into(), available: p.is_available() })
                .collect(),
        })
    }

    pub fn add_project(&self, path: &str) -> Result<Project> {
        let root = git::toplevel(Path::new(path))?;
        let name = root.file_name().map_or_else(|| "project".into(), |n| n.to_string_lossy().into_owned());
        let project = self.store().add_project(&name, &root.to_string_lossy())?;
        self.emit(Event::ProjectChanged(project.clone()));
        Ok(project)
    }

    pub fn projects(&self) -> Result<Vec<Project>> {
        Ok(self.store().projects()?)
    }

    pub fn remove_project(&self, project_id: i64) -> Result<()> {
        {
            let store = self.store();
            if !store.tasks(Some(project_id), false)?.is_empty() {
                return Err(Error::new(ErrorKind::InvalidParams, "project has active tasks; archive them first"));
            }
            store.remove_project(project_id)?;
        }
        self.emit(Event::ProjectRemoved { project_id });
        Ok(())
    }

    pub fn node_config(&self) -> Result<NodeConfigInfo> {
        node_settings::load(&self.paths)
    }

    pub fn set_node_config(&self, config: &NodeConfig) -> Result<()> {
        node_settings::save(&self.paths, config)
    }

    pub fn github_status(&self) -> GithubStatus {
        github::status(&self.options.gh_bin)
    }

    pub fn github_repos(&self, owner: &str) -> Result<Vec<GithubRepo>> {
        repo_source::check_name("owner", owner)?;
        github::repos(&self.options.gh_bin, owner)
    }

    fn new_repo_dir(&self, owner: &str, name: &str) -> Result<PathBuf> {
        let dir = node_settings::repos_dir(&self.paths)?.join(owner).join(name);
        if let Some(parent) = dir.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // create_dir is atomic, so concurrent requests cannot both claim (and later clean up) the same target.
        match std::fs::create_dir(&dir) {
            Ok(()) => Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(Error::new(ErrorKind::InvalidParams, format!("{} already exists", dir.display())))
            }
            Err(e) => Err(e.into()),
        }
    }

    pub fn clone_project(&self, source: &str) -> Result<Project> {
        let source = repo_source::parse_source(source)?;
        let target = self.new_repo_dir(&source.owner, &source.repo)?;
        let env = &self.options.git_env;
        let cloned = match &source.url {
            Some(url) => git::clone_url(url, &target, env),
            None if github::logged_in(&self.options.gh_bin) => {
                github::clone(&self.options.gh_bin, &source.owner, &source.repo, &target, env)
            }
            None => git::clone_url(&format!("https://github.com/{}/{}.git", source.owner, source.repo), &target, env),
        };
        if let Err(e) = cloned {
            // A half-cloned directory would block the next attempt.
            let _ = std::fs::remove_dir_all(&target);
            return Err(e);
        }
        self.add_project(&target.to_string_lossy())
    }

    pub fn create_project(&self, params: &ProjectCreateParams) -> Result<ProjectCreateResult> {
        repo_source::check_name("repository name", &params.name)?;
        let owner = match &params.github {
            Some(target) => {
                let status = self.github_status();
                if !status.logged_in {
                    return Err(Error::new(ErrorKind::InvalidParams, "the GitHub CLI is not logged in; run `gh auth login`"));
                }
                let user = status.login.as_deref().filter(|login| login.eq_ignore_ascii_case(&target.owner));
                let is_user = user.is_some();
                let Some(canonical) = user.or_else(|| status.orgs.iter().map(String::as_str).find(|org| org.eq_ignore_ascii_case(&target.owner))) else {
                    return Err(Error::new(ErrorKind::InvalidParams, format!("{} is not you or one of your organizations", target.owner)));
                };
                if is_user && target.visibility == Visibility::Internal {
                    return Err(Error::new(ErrorKind::InvalidParams, "internal visibility needs an organization owner"));
                }
                canonical.to_string()
            }
            None => LOCAL_OWNER.to_string(),
        };
        let dir = self.new_repo_dir(&owner, &params.name)?;
        if let Err(e) = git::init_with_readme(&dir, &params.name, &self.options.git_env) {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(e);
        }
        let project = self.add_project(&dir.to_string_lossy())?;
        let github_error = params
            .github
            .as_ref()
            .and_then(|target| github::create(&self.options.gh_bin, target, &owner, &params.name, &dir, &self.options.git_env).err())
            .map(|e| e.message);
        Ok(ProjectCreateResult { project, github_error })
    }

    pub fn create_task(self: &Arc<Self>, params: TaskCreateParams) -> Result<TaskCreateResult> {
        if let Some(name) = &params.agent {
            self.agent(name)?;
        }
        let project = self.store().project(params.project_id)?.ok_or_else(|| not_found("project", params.project_id))?;
        let repo = PathBuf::from(&project.path);
        let base = git::base_ref(&repo)?;
        let origin = git::remote_url(&repo, "origin");
        let (owner, repo_name) = node_settings::layout_owner_repo(origin.as_deref(), &project.name);
        let worktree_root = node_settings::worktrees_dir(&self.paths)?.join(owner).join(repo_name);
        let id = self.store().insert_task(project.id, &params.title, params.prompt.as_deref(), &base)?;
        let slug = slugify(id, &params.title);
        let branch = format!("asterism/{slug}");
        let worktree = worktree_root.join(&slug);
        if let Err(e) = git::add_worktree(&repo, &branch, &worktree, &base) {
            self.store().delete_task(id)?;
            return Err(e);
        }
        self.store().set_task_location(id, &slug, &branch, &worktree.to_string_lossy())?;
        let task = self.task(id)?;
        self.emit(Event::TaskChanged(task.clone()));

        let session = match params.agent {
            Some(name) => Some(self.start_session(SessionStartParams {
                task_id: id,
                kind: SessionKind::Agent { name },
                prompt: params.prompt,
            })?),
            None => None,
        };
        Ok(TaskCreateResult { task, session })
    }

    pub fn task(&self, id: i64) -> Result<Task> {
        self.store().task(id)?.ok_or_else(|| not_found("task", id))
    }

    pub fn tasks(&self, params: TaskListParams) -> Result<Vec<Task>> {
        Ok(self.store().tasks(params.project_id, params.include_archived)?)
    }

    pub fn archive_task(&self, task_id: i64, force: bool) -> Result<Task> {
        let task = self.task(task_id)?;
        if task.archived {
            return Ok(task);
        }
        let worktree = PathBuf::from(&task.worktree_path);
        if !force && git::is_dirty(&worktree)? {
            return Err(Error::new(
                ErrorKind::DirtyWorktree,
                format!("task {task_id} has uncommitted changes; archive with force to discard them"),
            ));
        }
        let sessions = self.store().sessions(Some(task_id))?;
        for stored in sessions {
            if let Ok(live) = self.live(stored.session.id) {
                let _ = live.pty.kill();
            }
        }
        let project = self.store().project(task.project_id)?.ok_or_else(|| not_found("project", task.project_id))?;
        git::remove_worktree(Path::new(&project.path), &worktree, force)?;
        self.store().set_task_archived(task_id)?;
        let task = self.task(task_id)?;
        self.emit(Event::TaskChanged(task.clone()));
        Ok(task)
    }

    pub fn diff(&self, task_id: i64) -> Result<TaskDiffResult> {
        let task = self.task(task_id)?;
        Ok(TaskDiffResult { patch: git::diff(Path::new(&task.worktree_path), &task.base_branch)? })
    }

    fn agent(&self, name: &str) -> Result<&'static AgentProfile> {
        agents::profile(name).filter(|p| p.is_available()).ok_or_else(|| {
            Error::new(ErrorKind::AgentUnavailable, format!("agent {name} is not installed on this node"))
        })
    }

    pub fn start_session(self: &Arc<Self>, params: SessionStartParams) -> Result<Session> {
        let task = self.task(params.task_id)?;
        if task.archived {
            return Err(Error::new(ErrorKind::InvalidParams, format!("task {} is archived", task.id)));
        }
        let argv = match &params.kind {
            SessionKind::Agent { name } => {
                agent_settings::start_argv(&self.paths, self.agent(name)?, params.prompt.as_deref())?
            }
            SessionKind::Shell => vec![std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into())],
            SessionKind::Command { argv } if !argv.is_empty() => argv.clone(),
            SessionKind::Command { .. } => {
                return Err(Error::new(ErrorKind::InvalidParams, "command argv must not be empty"));
            }
        };
        let id = self.store().insert_session(task.id, &params.kind, SessionStatus::Working)?;
        if let Err(e) = self.spawn_live(id, &task, argv, &params.kind) {
            self.store().set_session_status(id, SessionStatus::Exited)?;
            return Err(e);
        }
        let session = self.session(id)?;
        self.emit(Event::SessionChanged(session.clone()));
        Ok(session)
    }

    fn spawn_live(
        self: &Arc<Self>,
        id: i64,
        task: &Task,
        argv: Vec<String>,
        kind: &SessionKind,
    ) -> Result<()> {
        let policy = Config::load(&self.paths.config())?.env_policy(config::agent_key(kind));
        let bin_dir = std::env::current_exe()?.parent().map(Path::to_path_buf).unwrap_or_default();
        let inherited_path = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect::<Vec<_>>());
        let path = std::env::join_paths(std::iter::once(bin_dir.clone()).chain(inherited_path.unwrap_or_default()))
            .map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?;
        let fixed = [
            ("PATH".to_string(), path.to_string_lossy().into_owned()),
            ("ASTERISM_HOME".to_string(), self.paths.home.display().to_string()),
            ("ASTERISM_SOCKET".to_string(), self.paths.socket().display().to_string()),
            ("ASTERISM_CLI".to_string(), bin_dir.join("asterism").display().to_string()),
            ("ASTERISM_TASK".to_string(), task.id.to_string()),
            ("ASTERISM_SESSION".to_string(), id.to_string()),
        ];
        let inherited = std::env::vars_os().filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)));
        let env = config::session_env(inherited, &policy, &fixed);
        let pty = Pty::spawn(SpawnSpec {
            argv,
            cwd: PathBuf::from(&task.worktree_path),
            env,
            rows: DEFAULT_ROWS,
            cols: DEFAULT_COLS,
        })?;
        let (status_tx, mut status_rx) = watch::channel(SessionStatus::Working);
        let live = Arc::new(LiveSession {
            pty: pty.clone(),
            status: Arc::new(status_tx),
            hooks_active: Arc::new(AtomicBool::new(false)),
        });
        lock(&self.live).insert(id, live.clone());
        tokio::spawn(status::track(pty.clone(), agents::waiting_patterns(kind), live.status.clone(), live.hooks_active.clone()));

        let daemon = self.clone();
        tokio::spawn(async move {
            while status_rx.changed().await.is_ok() {
                let status = *status_rx.borrow_and_update();
                if status == SessionStatus::Exited {
                    // Stored before leaving `live` so `read` always finds one of the two.
                    let text = last_lines(&pty.history(), SCROLLBACK_LINES);
                    let _ = daemon.store().set_session_last_text(id, &text);
                    // Already taken by `remove_session`, which emits `session.removed`; nothing may follow.
                    if lock(&daemon.live).remove(&id).is_none() {
                        break;
                    }
                }
                let _ = daemon.store().set_session_status(id, status);
                daemon.emit(Event::SessionStatusChanged { session_id: id, status });
                if status == SessionStatus::Exited {
                    break;
                }
            }
        });
        Ok(())
    }

    fn live(&self, id: i64) -> Result<Arc<LiveSession>> {
        lock(&self.live)
            .get(&id)
            .cloned()
            .ok_or_else(|| Error::new(ErrorKind::NotFound, format!("session {id} is not running")))
    }

    pub fn session(&self, id: i64) -> Result<Session> {
        self.store().session(id)?.map(|s| s.session).ok_or_else(|| not_found("session", id))
    }

    pub fn sessions(&self, task_id: Option<i64>) -> Result<Vec<Session>> {
        Ok(self.store().sessions(task_id)?.into_iter().map(|s| s.session).collect())
    }

    pub fn agent_config(&self, agent: &str) -> Result<AgentConfig> {
        agent_settings::load(&self.paths, agent)
    }

    pub fn agent_config_raw(&self, agent: &str) -> Result<AgentConfigRaw> {
        agent_settings::load_raw(&self.paths, agent)
    }

    pub fn set_agent_config(&self, agent: &str, config: &AgentConfig) -> Result<()> {
        agent_settings::save(&self.paths, agent, config)
    }

    /// Stops the session if it still runs, then forgets it entirely.
    pub async fn remove_session(&self, id: i64) -> Result<()> {
        self.session(id)?;
        // Taken out first so the status forwarder sees it gone and reports nothing after `session.removed`.
        let live = lock(&self.live).remove(&id);
        if let Some(live) = live {
            let mut status = live.status.subscribe();
            let _ = live.pty.kill();
            let exited = tokio::time::timeout(REMOVE_GRACE, status.wait_for(|s| *s == SessionStatus::Exited))
                .await
                .is_ok_and(|r| r.is_ok());
            if !exited && !*live.pty.exited().borrow() {
                let _ = live.pty.force_kill();
            }
        }
        self.store().delete_session(id)?;
        self.emit(Event::SessionRemoved { session_id: id });
        Ok(())
    }

    pub fn kill_session(&self, id: i64) -> Result<()> {
        Ok(self.live(id)?.pty.kill()?)
    }

    pub async fn send(&self, params: SessionSendParams) -> Result<()> {
        let live = self.live(params.session_id)?;
        write_blocking(&live.pty, params.text.into_bytes()).await?;
        if params.submit {
            tokio::time::sleep(SUBMIT_DELAY).await;
            write_blocking(&live.pty, b"\r".to_vec()).await?;
            // Hook sessions stay idle until UserPromptSubmit arrives; a `wait --until idle` right after must not pass.
            if live.hooks_active.load(Ordering::Relaxed) {
                set_unless_exited(&live.status, SessionStatus::Working);
            }
        }
        Ok(())
    }

    pub fn resize(&self, params: SessionResizeParams) -> Result<()> {
        if params.rows == 0 || params.cols == 0 {
            return Err(Error::new(ErrorKind::InvalidParams, "rows and cols must be at least 1"));
        }
        Ok(self.live(params.session_id)?.pty.resize(params.rows, params.cols)?)
    }

    pub fn read(&self, params: SessionReadParams) -> Result<SessionReadResult> {
        let id = params.session_id;
        let text = match self.live(id) {
            Ok(live) => live.pty.history(),
            Err(_) => self.store().session_last_text(id)?.ok_or_else(|| not_found("session", id))?.unwrap_or_default(),
        };
        Ok(SessionReadResult { text: last_lines(&text, params.lines) })
    }

    pub fn attach(&self, session_id: i64) -> Result<(Snapshot, broadcast::Receiver<Vec<u8>>)> {
        Ok(self.live(session_id)?.pty.attach())
    }

    pub async fn wait(&self, params: SessionWaitParams) -> Result<SessionStatus> {
        let mut rx = match self.live(params.session_id) {
            Ok(live) => live.status.subscribe(),
            Err(_) => return Ok(self.session(params.session_id)?.status),
        };
        let until = params.until;
        let reached = rx.wait_for(|s| *s == until || *s == SessionStatus::Exited);
        let outcome = match params.timeout_ms {
            Some(ms) => tokio::time::timeout(Duration::from_millis(ms), reached).await.map_err(|_| {
                Error::new(
                    ErrorKind::Timeout,
                    format!("session {} did not reach {until:?} within {ms} ms", params.session_id),
                )
            })?,
            None => reached.await,
        };
        Ok(outcome.map(|s| *s).unwrap_or(SessionStatus::Exited))
    }

    pub fn hook(&self, params: SessionHookParams) -> Result<()> {
        let live = self.live(params.session_id)?;
        live.hooks_active.store(true, Ordering::Relaxed);
        set_unless_exited(&live.status, status::hook_status(params.event));
        if let Some(agent_ref) = params.agent_ref {
            self.store().set_session_agent_ref(params.session_id, &agent_ref)?;
        }
        Ok(())
    }

    pub fn recover(self: &Arc<Self>) -> Result<()> {
        let stored = self.store().sessions(None)?;
        for crate::store::StoredSession { session, agent_ref } in stored {
            if session.status == SessionStatus::Exited {
                continue;
            }
            let resumed = match self.task(session.task_id) {
                Ok(task) if !task.archived => {
                    match agent_settings::resume_argv(&self.paths, &session.kind, agent_ref.as_deref()) {
                        Ok(Some(argv)) => {
                            let spawned = self.spawn_live(session.id, &task, argv, &session.kind);
                            if let Err(e) = &spawned {
                                eprintln!("asterismd: could not resume session {}: {e}", session.id);
                            }
                            spawned.is_ok()
                        }
                        Ok(None) => false,
                        Err(e) => {
                            eprintln!("asterismd: could not resume session {}: {e}", session.id);
                            false
                        }
                    }
                }
                Ok(_) => false,
                Err(e) => {
                    eprintln!("asterismd: could not resume session {}: task lookup failed: {e}", session.id);
                    false
                }
            };
            if !resumed {
                self.store().set_session_status(session.id, SessionStatus::Exited)?;
            }
        }
        Ok(())
    }
}

fn set_unless_exited(status: &watch::Sender<SessionStatus>, next: SessionStatus) {
    status.send_if_modified(|current| {
        let changed = *current != next && *current != SessionStatus::Exited;
        if changed {
            *current = next;
        }
        changed
    });
}

async fn write_blocking(pty: &Arc<Pty>, data: Vec<u8>) -> Result<()> {
    let pty = pty.clone();
    tokio::task::spawn_blocking(move || pty.write(&data))
        .await
        .map_err(|e| Error::new(ErrorKind::Internal, format!("pty write task failed: {e}")))??;
    Ok(())
}

fn not_found(what: &str, id: i64) -> Error {
    Error::new(ErrorKind::NotFound, format!("{what} {id} not found"))
}

pub fn slugify(id: i64, title: &str) -> String {
    let mut slug = String::new();
    for c in title.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug: String = slug.chars().take(SLUG_MAX).collect();
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        id.to_string()
    } else {
        format!("{id}-{slug}")
    }
}

pub fn last_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.trim_end().lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_branch_safe_and_unique_by_id() {
        assert_eq!(slugify(1, "Fix login bug"), "1-fix-login-bug");
        assert_eq!(slugify(2, "  Ünïcode & spaces!! "), "2-n-code-spaces");
        assert_eq!(slugify(3, "!!!"), "3");
        assert_eq!(slugify(4, ""), "4");
        assert_eq!(slugify(5, "a/b..c~d^e:f"), "5-a-b-c-d-e-f");
        let long = slugify(6, &"x".repeat(200));
        assert_eq!(long.len(), "6-".len() + SLUG_MAX);
        assert_eq!(slugify(7, &format!("{}-tail", "y".repeat(39))), format!("7-{}", "y".repeat(39)));
    }

    #[test]
    fn last_lines_trims_trailing_blank_screen() {
        assert_eq!(last_lines("a\nb\nc\n\n\n", 2), "b\nc");
        assert_eq!(last_lines("a", 10), "a");
        assert_eq!(last_lines("", 3), "");
    }
}
