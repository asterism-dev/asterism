use std::fs::{self, DirBuilder, Permissions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Paths {
    pub home: PathBuf,
}

impl Paths {
    pub fn from_env() -> Self {
        let home = std::env::var_os("ASTERISM_HOME").map(PathBuf::from).unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".asterism")
        });
        Self { home }
    }

    pub fn socket(&self) -> PathBuf {
        self.home.join("asterismd.sock")
    }

    pub fn db(&self) -> PathBuf {
        self.home.join("state.db")
    }

    pub fn worktrees(&self) -> PathBuf {
        self.home.join("worktrees")
    }

    pub fn claude_settings(&self) -> PathBuf {
        self.home.join("claude-settings.json")
    }

    pub fn lock(&self) -> PathBuf {
        self.home.join("asterismd.lock")
    }

    pub fn log(&self) -> PathBuf {
        self.home.join("asterismd.log")
    }

    pub fn ensure_dirs(&self) -> io::Result<()> {
        // The socket grants full control over this user's sessions, so only the owner may reach it.
        DirBuilder::new().recursive(true).mode(0o700).create(&self.home)?;
        fs::set_permissions(&self.home, Permissions::from_mode(0o700))?;
        fs::create_dir_all(self.worktrees())
    }
}
