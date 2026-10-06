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

    pub fn config(&self) -> PathBuf {
        self.home.join("config.toml")
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

    pub fn agent_dir(&self, agent: &str) -> PathBuf {
        self.home.join("agents").join(agent)
    }

    pub fn agent_mcp(&self, agent: &str) -> PathBuf {
        self.agent_dir(agent).join("mcp.json")
    }

    pub fn agent_hooks(&self, agent: &str) -> PathBuf {
        self.agent_dir(agent).join("hooks.json")
    }

    pub fn plugins_dir(&self) -> PathBuf {
        self.home.join("plugins")
    }

    pub fn plugin_links(&self) -> PathBuf {
        self.plugins_dir().join("links.toml")
    }

    pub fn plugin_data(&self, plugin: &str) -> PathBuf {
        self.plugins_dir().join("data").join(plugin)
    }

    pub fn plugin_stores_file(&self) -> PathBuf {
        self.plugins_dir().join("stores.toml")
    }

    pub fn plugin_stores_dir(&self) -> PathBuf {
        self.plugins_dir().join("stores")
    }

    pub fn plugin_cache(&self) -> PathBuf {
        self.plugins_dir().join("cache").join("git")
    }

    pub fn plugins_installed(&self) -> PathBuf {
        self.plugins_dir().join("installed")
    }

    pub fn plugin_installed_file(&self) -> PathBuf {
        self.plugins_dir().join("installed.toml")
    }

    pub fn secrets(&self) -> PathBuf {
        self.home.join("secrets.toml")
    }
}
