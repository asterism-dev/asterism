use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::Duration;

use asterism_plugin::protocol::{self, CloneParams, CreateRemoteParams, LaunchMode, ListReposParams, PrepareParams, PrepareResult, ResolveOwnerParams, ResolveOwnerResult};
use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::*;
use asterism_proto::PROTO_VERSION;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use tokio::sync::{broadcast, watch, Notify};

use crate::agent_settings;
use crate::agents;
use crate::config::{self, Config};
use crate::error::{Error, Result};
use crate::git::GitEnv;
use crate::paths::Paths;
use crate::plugins::manifest::{self, AgentDecl, LaunchKind};
use crate::plugins::process::{self, HostFn};
use crate::plugins::registry::{self, Plugin, Status};
use crate::plugins::catalog::{self, StoreConfig};
use crate::plugins::{install, settings, source, store_ops, PluginSet, Runtime, STORE_LOCK};
use crate::proc_stats::{self, CpuTracker};
use crate::session::{Pty, Snapshot, SpawnSpec, SCROLLBACK_LINES};
use crate::store::Store;
use crate::node_settings::LOCAL_OWNER;
use crate::{git, lock, node_settings, repo_source, status};

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

/// Overridable external tools; tests inject an isolated git environment.
pub struct DaemonOptions {
    pub git_env: Vec<(String, String)>,
    /// Where built-in plugin backends live; defaults to the daemon's own directory.
    pub builtin_plugins_dir: Option<PathBuf>,
    /// Extra environment for plugin backends, after the inherited one.
    pub plugin_env: Vec<(String, String)>,
    pub plugin_call_timeout: Duration,
    pub plugin_idle: Duration,
}

impl Default for DaemonOptions {
    fn default() -> Self {
        Self {
            git_env: Vec::new(),
            builtin_plugins_dir: None,
            plugin_env: Vec::new(),
            plugin_call_timeout: process::CALL_TIMEOUT,
            plugin_idle: process::IDLE_TIMEOUT,
        }
    }
}

/// The daemon's directory (bundled sidecars) followed by the inherited PATH.
fn tool_path() -> Result<(PathBuf, String)> {
    let bin_dir = std::env::current_exe()?.parent().map(Path::to_path_buf).unwrap_or_default();
    let inherited = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).unwrap_or_default();
    let path = std::env::join_paths(std::iter::once(bin_dir.clone()).chain(inherited))
        .map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?;
    Ok((bin_dir, path.to_string_lossy().into_owned()))
}

const PLUGIN_FIXED_ENV: &[&str] = &["PATH", "ASTERISM_HOME", "ASTERISM_SOCKET", "ASTERISM_CLI"];

fn plugin_env(paths: &Paths, extra: &[(String, String)]) -> Result<Vec<(String, String)>> {
    let (bin_dir, path) = tool_path()?;
    let mut env: Vec<(String, String)> = std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
        .filter(|(k, _)| !PLUGIN_FIXED_ENV.contains(&k.as_str()) && !config::removed_by_default(k))
        .collect();
    env.extend([
        ("PATH".to_string(), path),
        ("ASTERISM_HOME".to_string(), paths.home.display().to_string()),
        ("ASTERISM_SOCKET".to_string(), paths.socket().display().to_string()),
        ("ASTERISM_CLI".to_string(), bin_dir.join("asterism").display().to_string()),
    ]);
    env.extend(extra.iter().cloned());
    Ok(env)
}

pub const STORE_REFRESH_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, Default)]
struct StoreStatus {
    last_refreshed: Option<i64>,
    last_error: Option<String>,
}

fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

pub struct Daemon {
    options: DaemonOptions,
    paths: Paths,
    store: Mutex<Store>,
    live: Mutex<HashMap<i64, Arc<LiveSession>>>,
    events: broadcast::Sender<Event>,
    shutdown: Notify,
    cpu: Mutex<CpuTracker>,
    plugins: RwLock<Arc<PluginSet>>,
    host: HostFn,
    runtime: Runtime,
    builtin_dir: PathBuf,
    store_status: Mutex<BTreeMap<String, StoreStatus>>,
}

impl Daemon {
    pub fn new(paths: Paths) -> Result<Arc<Self>> {
        Self::with_options(paths, DaemonOptions::default())
    }

    pub fn with_options(paths: Paths, options: DaemonOptions) -> Result<Arc<Self>> {
        paths.ensure_dirs()?;
        // Moved to plugins/data/claude/ with the Claude plugin.
        let _ = std::fs::remove_file(paths.home.join("claude-settings.json"));
        let store = Store::open(&paths.db())?;
        let (events, _) = broadcast::channel(1024);
        let runtime = Runtime {
            env: plugin_env(&paths, &options.plugin_env)?,
            call_timeout: options.plugin_call_timeout,
            idle: options.plugin_idle,
        };
        let builtin_dir = match &options.builtin_plugins_dir {
            Some(dir) => dir.clone(),
            None => tool_path()?.0,
        };
        Ok(Arc::new_cyclic(move |daemon| {
            let host = crate::rpc::host_fn(daemon.clone());
            let plugins = PluginSet::load(&paths, &builtin_dir, &runtime, &host);
            Self {
                options,
                paths,
                store: Mutex::new(store),
                live: Mutex::new(HashMap::new()),
                events,
                shutdown: Notify::new(),
                cpu: Mutex::new(CpuTracker::default()),
                plugins: RwLock::new(Arc::new(plugins)),
                host,
                runtime,
                builtin_dir,
                store_status: Mutex::new(BTreeMap::new()),
            }
        }))
    }

    pub fn plugin_set(&self) -> Arc<PluginSet> {
        self.plugins.read().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }

    pub fn call_timeout(&self) -> Duration {
        self.runtime.call_timeout
    }

    pub async fn plugin_call(&self, plugin: &str, method: &str, params: Value, timeout: Option<Duration>) -> Result<Value> {
        let set = self.plugin_set();
        let unavailable = |reason: String| Error::new(ErrorKind::PluginError, format!("plugin {plugin} {reason}"));
        let entry = set.registry.get(plugin).ok_or_else(|| unavailable("is not installed".into()))?;
        if let Status::Broken(reason) = &entry.status {
            return Err(unavailable(format!("is broken: {reason}")));
        }
        if let Some(manifest) = &entry.manifest {
            let missing = settings::missing(&self.paths, plugin, &manifest.settings)?;
            if !missing.is_empty() {
                return Err(Error::new(
                    ErrorKind::PluginError,
                    format!("{plugin}: {} is not set, see Settings → Plugins", missing.join(", ")),
                ));
            }
        }
        let backend = set.backend(plugin).cloned().ok_or_else(|| unavailable("has no backend".into()))?;
        backend.call(method, params, timeout).await
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

    fn plugin_info(&self, set: &PluginSet, plugin: &Plugin) -> PluginInfo {
        let state = match &plugin.status {
            Status::Broken(reason) => PluginState::Broken { reason: reason.clone() },
            Status::Ok => match set.backend(&plugin.name).and_then(|b| b.failing()) {
                Some(reason) => PluginState::Failing { reason },
                None => match plugin.manifest.as_ref().map(|m| settings::missing(&self.paths, &plugin.name, &m.settings)) {
                    Some(Ok(missing)) if !missing.is_empty() => PluginState::NeedsSetup { missing },
                    Some(Err(e)) => PluginState::Broken { reason: e.message },
                    _ => PluginState::Ok,
                },
            },
            Status::Disabled => PluginState::Disabled,
        };
        let manifest = plugin.manifest.as_ref();
        let entry = (plugin.origin == PluginOrigin::Installed).then(|| set.installed.plugins.get(&plugin.name)).flatten();
        PluginInfo {
            name: plugin.name.clone(),
            version: manifest.map(|m| m.version.clone()),
            description: manifest.map(|m| m.description.clone()).unwrap_or_default(),
            origin: plugin.origin,
            path: plugin.dir.display().to_string(),
            capabilities: plugin.capabilities(),
            permissions: manifest.map(|m| m.permissions.clone()).unwrap_or_default(),
            state,
            backend: plugin.backend_command(),
            store: entry.map(|e| e.store.clone()),
            update_available: false,
            previous_version: entry.and_then(|e| e.previous.clone()),
        }
    }

    pub fn plugin_list(&self) -> Result<Vec<PluginInfo>> {
        let set = self.plugin_set();
        Ok(set.registry.plugins().iter().map(|p| self.plugin_info(&set, p)).collect())
    }

    /// Runs a store/install operation off the async runtime under the global store lock.
    pub(crate) async fn plugin_op<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Paths, &GitEnv) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let (paths, env) = (self.paths.clone(), self.options.git_env.clone());
        tokio::task::spawn_blocking(move || {
            // ponytail: one lock for every store/install operation, git fetches included; split it if refreshes start blocking installs.
            let _guard = crate::lock(&STORE_LOCK);
            f(&paths, &env)
        })
        .await
        .map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?
    }

    fn set_store_status<T>(&self, name: &str, result: &Result<T>) {
        let mut status = lock(&self.store_status);
        let entry = status.entry(name.to_string()).or_default();
        match result {
            Ok(_) => {
                entry.last_refreshed = Some(unix_now());
                entry.last_error = None;
            }
            Err(e) => entry.last_error = Some(e.message.clone()),
        }
    }

    pub fn store_list(&self) -> StoreList {
        let file = match store_ops::load(&self.paths) {
            Ok(file) => file,
            Err(e) => return StoreList { error: Some(e.message), ..Default::default() },
        };
        let status = lock(&self.store_status);
        let stores = file
            .stores
            .iter()
            .map(|s| {
                let st = status.get(&s.name).cloned().unwrap_or_default();
                StoreInfo {
                    name: s.name.clone(),
                    source: s.source.clone(),
                    official: s.official,
                    last_refreshed: st.last_refreshed,
                    last_error: st.last_error,
                    plugin_count: source::read_index(&store_ops::store_dir(&self.paths, s)).map_or(0, |i| i.plugins.len()),
                }
            })
            .collect();
        StoreList { auto_update: file.auto_update, stores, error: None }
    }

    pub async fn store_add(&self, source_url: &str) -> Result<StoreInfo> {
        let source_url = source_url.trim().to_string();
        let added = self.plugin_op(move |paths, env| store_ops::add_store(paths, &source_url, env)).await?;
        self.set_store_status(&added.name, &Ok(()));
        self.emit(Event::StoresChanged {});
        self.store_list()
            .stores
            .into_iter()
            .find(|s| s.name == added.name)
            .ok_or_else(|| Error::new(ErrorKind::Internal, "added store vanished"))
    }

    pub async fn store_remove(&self, name: &str, uninstall_plugins: bool) -> Result<()> {
        let store = name.to_string();
        let uninstalled = self
            .plugin_op(move |paths, _| {
                store_ops::find_store(&store_ops::load(paths)?, &store)?;
                let owned: Vec<String> =
                    install::load(paths)?.plugins.into_iter().filter(|(_, e)| e.store == store).map(|(n, _)| n).collect();
                if !owned.is_empty() && !uninstall_plugins {
                    return Err(Error::new(
                        ErrorKind::InvalidParams,
                        format!("store {store} provides installed plugins ({}); remove them together with the store", owned.join(", ")),
                    ));
                }
                for plugin in &owned {
                    install::uninstall(paths, plugin)?;
                }
                store_ops::remove_store(paths, &store)?;
                Ok(owned)
            })
            .await?;
        lock(&self.store_status).remove(name);
        self.emit(Event::StoresChanged {});
        if !uninstalled.is_empty() {
            self.reload_plugins(None).await?;
        }
        Ok(())
    }

    /// Refreshes one store (its error is returned) or all of them (errors are recorded and logged).
    pub async fn refresh_stores(&self, name: Option<&str>) -> Result<()> {
        let file = store_ops::load(&self.paths)?;
        let targets: Vec<StoreConfig> = file.stores.iter().filter(|s| name.is_none_or(|n| n == s.name)).cloned().collect();
        if let (Some(name), true) = (name, targets.is_empty()) {
            return Err(Error::new(ErrorKind::NotFound, format!("no store named {name}")));
        }
        let mut failed = None;
        for store in targets {
            let config = store.clone();
            let result = self
                .plugin_op(move |paths, env| {
                    // Skip a store removed since the snapshot so its checkout is not re-created.
                    if !store_ops::load(paths)?.stores.iter().any(|s| s.name == config.name) {
                        return Ok(false);
                    }
                    store_ops::sync_store(paths, &config, env).map(|_| true)
                })
                .await;
            if matches!(result, Ok(false)) {
                continue;
            }
            self.set_store_status(&store.name, &result);
            if let Err(e) = result {
                eprintln!("asterismd: refreshing store {}: {}", store.name, e.message);
                failed = Some(e);
            }
        }
        self.emit(Event::StoresChanged {});
        match (name, failed) {
            (Some(_), Some(e)) => Err(e),
            _ => Ok(()),
        }
    }

    pub async fn set_auto_update(&self, enabled: bool) -> Result<()> {
        self.plugin_op(move |paths, _| {
            let mut file = store_ops::load(paths)?;
            file.auto_update = enabled;
            catalog::save_stores(&paths.plugin_stores_file(), &file)
        })
        .await?;
        self.emit(Event::StoresChanged {});
        Ok(())
    }

    pub async fn store_refresh_loop(self: Arc<Self>) {
        loop {
            if let Err(e) = self.refresh_stores(None).await {
                eprintln!("asterismd: store refresh failed: {}", e.message);
            }
            tokio::time::sleep(STORE_REFRESH_INTERVAL).await;
        }
    }

    /// Re-reads every manifest; backends restart lazily on their next call.
    pub async fn reload_plugins(&self, name: Option<&str>) -> Result<()> {
        if let Some(name) = name {
            if self.plugin_set().registry.get(name).is_none() {
                return Err(Error::new(ErrorKind::NotFound, format!("plugin {name} is not installed")));
            }
        }
        let fresh = Arc::new(PluginSet::load(&self.paths, &self.builtin_dir, &self.runtime, &self.host));
        let old = std::mem::replace(&mut *self.plugins.write().unwrap_or_else(std::sync::PoisonError::into_inner), fresh);
        old.stop_all().await;
        self.emit(Event::PluginsChanged {});
        Ok(())
    }

    pub async fn plugin_link(&self, path: &str) -> Result<PluginInfo> {
        let invalid = |message: String| Error::new(ErrorKind::InvalidParams, message);
        let dir = node_settings::expand_home(path)?.canonicalize().map_err(|e| invalid(format!("{path}: {e}")))?;
        let text = std::fs::read_to_string(dir.join("plugin.toml")).map_err(|e| invalid(format!("no plugin.toml in {}: {e}", dir.display())))?;
        let manifest = manifest::parse(&text).map_err(|e| invalid(format!("{}: {e}", dir.join("plugin.toml").display())))?;
        {
            let _guard = crate::lock(&agent_settings::SAVE_LOCK);
            let mut links = registry::load_links(&self.paths.plugin_links()).map_err(invalid)?;
            links.insert(manifest.name.clone(), dir);
            registry::save_links(&self.paths.plugin_links(), &links)?;
        }
        self.reload_plugins(None).await?;
        let set = self.plugin_set();
        let plugin = set.registry.get(&manifest.name).ok_or_else(|| Error::new(ErrorKind::Internal, "linked plugin vanished"))?;
        Ok(self.plugin_info(&set, plugin))
    }

    pub async fn plugin_unlink(&self, name: &str) -> Result<()> {
        {
            let _guard = crate::lock(&agent_settings::SAVE_LOCK);
            let mut links = registry::load_links(&self.paths.plugin_links()).map_err(|e| Error::new(ErrorKind::InvalidParams, e))?;
            if links.remove(name).is_none() {
                return Err(Error::new(ErrorKind::NotFound, format!("plugin {name} is not linked")));
            }
            registry::save_links(&self.paths.plugin_links(), &links)?;
        }
        self.reload_plugins(None).await
    }

    fn plugin_schema(&self, name: &str) -> Result<Vec<SettingSpec>> {
        let set = self.plugin_set();
        let plugin = set.registry.get(name).ok_or_else(|| Error::new(ErrorKind::NotFound, format!("plugin {name} is not installed")))?;
        let manifest = plugin
            .manifest
            .as_ref()
            .ok_or_else(|| Error::new(ErrorKind::InvalidParams, format!("plugin {name} is broken and has no settings")))?;
        Ok(manifest.settings.clone())
    }

    pub fn plugin_settings(&self, name: &str) -> Result<PluginSettings> {
        settings::view(&self.paths, name, &self.plugin_schema(name)?)
    }

    pub async fn set_plugin_settings(&self, name: &str, values: &BTreeMap<String, Value>) -> Result<()> {
        let schema = self.plugin_schema(name)?;
        settings::save(&self.paths, name, &schema, values)?;
        if let Some(backend) = self.plugin_set().backend(name).cloned() {
            backend.update_settings(settings::resolved(&self.paths, name, &schema)?).await;
        }
        self.emit(Event::PluginsChanged {});
        Ok(())
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
            agents: self.agent_infos(),
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

    pub fn forges(&self) -> Vec<ForgeInfo> {
        let set = self.plugin_set();
        set.registry
            .forges()
            .map(|(plugin, forge)| ForgeInfo {
                id: forge.id.clone(),
                display_name: forge.display_name.clone(),
                hosts: forge.hosts.clone(),
                plugin: plugin.name.clone(),
            })
            .collect()
    }

    async fn forge_call<P: Serialize, R: DeserializeOwned>(&self, forge: &str, method: &str, params: P, timeout: Option<Duration>) -> Result<R> {
        let plugin = self
            .plugin_set()
            .registry
            .forge(forge)
            .map(|(plugin, _)| plugin.name.clone())
            .ok_or_else(|| Error::new(ErrorKind::PluginError, format!("no forge {forge:?} is installed")))?;
        let params = serde_json::to_value(params).map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?;
        let reply = self.plugin_call(&plugin, method, params, timeout).await?;
        serde_json::from_value(reply).map_err(|e| Error::new(ErrorKind::PluginError, format!("{plugin}: unexpected {method} reply: {e}")))
    }

    pub async fn forge_status(&self, forge: &str) -> Result<ForgeStatus> {
        self.forge_call(forge, protocol::method::FORGE_STATUS, serde_json::json!({}), Some(self.call_timeout())).await
    }

    pub async fn forge_repos(&self, forge: &str, owner: &str) -> Result<Vec<ForgeRepo>> {
        repo_source::check_name("owner", owner)?;
        let params = ListReposParams { owner: owner.to_string() };
        self.forge_call(forge, protocol::method::FORGE_LIST_REPOS, params, Some(self.call_timeout())).await
    }

    fn resolve_forge(&self, explicit: Option<&str>) -> Result<String> {
        let (id, origin) = match explicit {
            Some(id) => (id.to_string(), ""),
            None => match Config::load(&self.paths.config())?.default_forge {
                Some(id) => (id, " in config.toml"),
                None => {
                    let first = self.plugin_set().registry.forges().next().map(|(_, forge)| forge.id.clone());
                    return first.ok_or_else(|| Error::new(ErrorKind::InvalidParams, "no forge is installed; clone by URL instead"));
                }
            },
        };
        if self.plugin_set().registry.forge(&id).is_none() {
            return Err(Error::new(ErrorKind::InvalidParams, format!("{}{id:?}{origin} is not an installed forge", if origin.is_empty() { "forge " } else { "default_forge " })));
        }
        Ok(id)
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

    pub async fn clone_project(self: &Arc<Self>, source: &str, forge: Option<&str>) -> Result<Project> {
        let source = repo_source::parse_source(source)?;
        let target = self.new_repo_dir(&source.owner, &source.repo)?;
        let cloned = match &source.url {
            Some(url) => self.git_clone(url.clone(), target.clone()).await,
            None => self.clone_shorthand(forge, &source.owner, &source.repo, &target).await,
        };
        if let Err(e) = cloned {
            // A half-cloned directory would block the next attempt.
            let _ = std::fs::remove_dir_all(&target);
            return Err(e);
        }
        self.add_project(&target.to_string_lossy())
    }

    async fn git_clone(&self, url: String, target: PathBuf) -> Result<()> {
        let env = self.options.git_env.clone();
        tokio::task::spawn_blocking(move || git::clone_url(&url, &target, &env))
            .await
            .map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?
    }

    /// `owner/repo` goes to the default forge: its own clone when signed in, else anonymous HTTPS.
    async fn clone_shorthand(&self, forge: Option<&str>, owner: &str, repo: &str, target: &Path) -> Result<()> {
        let forge = self.resolve_forge(forge)?;
        if forge_clone_wanted(self.forge_status(&forge).await)? {
            let params = CloneParams {
                owner: owner.to_string(),
                repo: repo.to_string(),
                target: target.display().to_string(),
                git_env: git::clone_env(&self.options.git_env),
            };
            return self.forge_call::<_, Value>(&forge, protocol::method::FORGE_CLONE, params, None).await.map(|_| ());
        }
        let host = self.plugin_set().registry.forge(&forge).and_then(|(_, f)| f.hosts.first().cloned());
        let host = host.ok_or_else(|| Error::new(ErrorKind::InvalidParams, format!("forge {forge} has no host to clone from")))?;
        self.git_clone(format!("https://{host}/{owner}/{repo}.git"), target.to_path_buf()).await
    }

    pub async fn create_project(self: &Arc<Self>, params: &ProjectCreateParams) -> Result<ProjectCreateResult> {
        repo_source::check_name("repository name", &params.name)?;
        let owner = match &params.remote {
            Some(target) => {
                let resolve = ResolveOwnerParams { owner: target.owner.clone(), visibility: target.visibility };
                let resolved: ResolveOwnerResult =
                    self.forge_call(&target.forge, protocol::method::FORGE_RESOLVE_OWNER, resolve, Some(self.call_timeout())).await?;
                // The plugin's answer names a directory.
                repo_source::check_name("owner", &resolved.owner)?;
                resolved.owner
            }
            None => LOCAL_OWNER.to_string(),
        };
        let dir = self.new_repo_dir(&owner, &params.name)?;
        let (init_dir, name, env) = (dir.clone(), params.name.clone(), self.options.git_env.clone());
        let initialized = tokio::task::spawn_blocking(move || git::init_with_readme(&init_dir, &name, &env))
            .await
            .map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?;
        if let Err(e) = initialized {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(e);
        }
        let project = self.add_project(&dir.to_string_lossy())?;
        let remote_error = match &params.remote {
            Some(target) => {
                let create = CreateRemoteParams {
                    owner,
                    name: params.name.clone(),
                    visibility: target.visibility,
                    dir: dir.display().to_string(),
                    git_env: git::clone_env(&self.options.git_env),
                };
                self.forge_call::<_, Value>(&target.forge, protocol::method::FORGE_CREATE_REMOTE, create, None).await.err().map(|e| e.message)
            }
            None => None,
        };
        Ok(ProjectCreateResult { project, remote_error })
    }

    pub async fn create_task(self: &Arc<Self>, params: TaskCreateParams) -> Result<TaskCreateResult> {
        if let Some(name) = &params.agent {
            self.ensure_agent_available(name)?;
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
            }).await?),
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

    /// Stops the task's sessions; worktree, uncommitted changes and branch stay.
    pub fn archive_task(&self, task_id: i64) -> Result<Task> {
        let task = self.task(task_id)?;
        if task.archived {
            return Ok(task);
        }
        for stored in self.store().sessions(Some(task_id))? {
            if let Ok(live) = self.live(stored.session.id) {
                let _ = live.pty.kill();
            }
        }
        self.store().set_task_archived(task_id)?;
        let task = self.task(task_id)?;
        self.emit(Event::TaskChanged(task.clone()));
        Ok(task)
    }

    pub fn restore_task(&self, task_id: i64) -> Result<Task> {
        let task = self.task(task_id)?;
        if !task.archived {
            return Ok(task);
        }
        let worktree = PathBuf::from(&task.worktree_path);
        let repo = self.project_path(task.project_id)?;
        if worktree.exists() {
            let listed = git::worktrees(&repo)?.iter().any(|w| same_path(&w.path, &task.worktree_path));
            if !listed {
                return Err(Error::new(
                    ErrorKind::InvalidParams,
                    format!("{} exists but is not a worktree of this project; move it away to restore", task.worktree_path),
                ));
            }
        } else if !git::branch_exists(&repo, &task.branch) {
            return Err(Error::new(ErrorKind::NotFound, format!("branch {} no longer exists", task.branch)));
        } else {
            // Tasks archived before archiving kept worktrees get theirs back from the branch.
            git::add_existing_worktree(&repo, &worktree, &task.branch)?;
        }
        self.store().set_task_active(task_id)?;
        let task = self.task(task_id)?;
        self.emit(Event::TaskChanged(task.clone()));
        Ok(task)
    }

    pub fn delete_check(&self, task_id: i64) -> Result<TaskDeleteCheck> {
        let task = self.task(task_id)?;
        let repo = self.project_path(task.project_id)?;
        let worktree = Path::new(&task.worktree_path);
        let branch_exists = git::branch_exists(&repo, &task.branch);
        Ok(TaskDeleteCheck {
            dirty: worktree.exists() && git::is_dirty(worktree).unwrap_or(false),
            unmerged_commits: if branch_exists { git::unmerged_commits(&repo, &task.base_branch, &task.branch) } else { 0 },
            branch: task.branch,
            branch_exists,
        })
    }

    pub fn delete_task(&self, task_id: i64, delete_branch: bool) -> Result<TaskDeleteResult> {
        let task = self.task(task_id)?;
        let repo = self.project_path(task.project_id)?;
        let sessions = self.store().sessions(Some(task_id))?;
        for stored in &sessions {
            // Taken out first so the status forwarder sees it gone and reports nothing after `session.removed`.
            let live = lock(&self.live).remove(&stored.session.id);
            if let Some(live) = live {
                let _ = live.pty.force_kill();
            }
        }
        let mut warnings = Vec::new();
        let worktree = Path::new(&task.worktree_path);
        if worktree.exists() {
            if let Err(e) = git::remove_worktree(&repo, worktree, true) {
                warnings.push(e.message);
            }
        }
        if delete_branch && git::branch_exists(&repo, &task.branch) {
            if let Err(e) = git::delete_branch(&repo, &task.branch) {
                warnings.push(e.message);
            }
        }
        {
            let store = self.store();
            store.delete_task_sessions(task_id)?;
            store.delete_task(task_id)?;
        }
        for stored in sessions {
            self.emit(Event::SessionRemoved { session_id: stored.session.id });
        }
        self.emit(Event::TaskRemoved { task_id });
        Ok(TaskDeleteResult { warning: (!warnings.is_empty()).then(|| warnings.join("; ")) })
    }

    pub fn project_worktrees(&self, project_id: i64) -> Result<Vec<Worktree>> {
        let repo = self.project_path(project_id)?;
        let tasks = self.store().tasks(Some(project_id), true)?;
        Ok(git::worktrees(&repo)?
            .into_iter()
            .map(|entry| {
                let task = tasks.iter().find(|t| same_path(&t.worktree_path, &entry.path));
                Worktree {
                    task_id: task.map(|t| t.id),
                    base_branch: task.map(|t| t.base_branch.clone()),
                    path: entry.path,
                    head: entry.head,
                    branch: entry.branch,
                    is_main: entry.is_main,
                    locked: entry.locked,
                    prunable: entry.prunable,
                }
            })
            .collect())
    }

    pub fn project_worktree_sizes(&self, project_id: i64) -> Result<Vec<WorktreeSize>> {
        let repo = self.project_path(project_id)?;
        Ok(git::worktrees(&repo)?
            .into_iter()
            .map(|entry| WorktreeSize { bytes: git::dir_size(Path::new(&entry.path)), path: entry.path })
            .collect())
    }

    pub fn remove_worktree(&self, project_id: i64, path: &str) -> Result<()> {
        let repo = self.project_path(project_id)?;
        let listed = self.project_worktrees(project_id)?;
        let Some(worktree) = listed.iter().find(|w| same_path(&w.path, path)) else {
            return Err(Error::new(ErrorKind::InvalidParams, format!("{path} is not a worktree of this project")));
        };
        if worktree.is_main || worktree.task_id.is_some() {
            return Err(Error::new(ErrorKind::InvalidParams, format!("{path} is the main checkout or belongs to a task")));
        }
        git::remove_worktree(&repo, Path::new(&worktree.path), true)
    }

    pub fn prune_worktrees(&self, project_id: i64) -> Result<()> {
        git::prune_worktrees(&self.project_path(project_id)?)
    }

    fn project_path(&self, project_id: i64) -> Result<PathBuf> {
        let project = self.store().project(project_id)?.ok_or_else(|| not_found("project", project_id))?;
        Ok(PathBuf::from(project.path))
    }

    pub fn diff(&self, task_id: i64) -> Result<TaskDiffResult> {
        let task = self.task(task_id)?;
        Ok(TaskDiffResult { patch: git::diff(Path::new(&task.worktree_path), &task.base_branch)? })
    }

    pub fn agent_infos(&self) -> Vec<AgentInfo> {
        let set = self.plugin_set();
        set.registry
            .agents()
            .map(|(plugin, agent)| AgentInfo {
                name: agent.id.clone(),
                available: agents::on_path(&agent.binary),
                display_name: agent.display_name().to_string(),
                settings: agent.settings.clone(),
                plugin: plugin.name.clone(),
            })
            .collect()
    }

    fn agent_decl(&self, name: &str) -> Result<(String, AgentDecl)> {
        let set = self.plugin_set();
        let (plugin, decl) = set
            .registry
            .agent(name)
            .ok_or_else(|| Error::new(ErrorKind::AgentUnavailable, format!("agent {name} is not installed on this node")))?;
        Ok((plugin.name.clone(), decl.clone()))
    }

    fn ensure_agent_available(&self, name: &str) -> Result<()> {
        let (_, decl) = self.agent_decl(name)?;
        if agents::on_path(&decl.binary) {
            Ok(())
        } else {
            Err(Error::new(ErrorKind::AgentUnavailable, format!("agent {name} is not installed on this node")))
        }
    }

    /// The argv and extra env for an agent session; `None` when the agent cannot resume.
    pub async fn agent_argv(
        &self,
        name: &str,
        mode: LaunchMode,
        prompt: Option<&str>,
        agent_ref: Option<&str>,
    ) -> Result<Option<(Vec<String>, Vec<(String, String)>)>> {
        let (plugin, decl) = self.agent_decl(name)?;
        let settings = agent_settings::launch_settings(&self.paths, name, mode == LaunchMode::Resume)?;
        match decl.launch {
            LaunchKind::Static => Ok(agents::static_argv(&decl, mode, &settings.args, prompt, agent_ref).map(|argv| (argv, Vec::new()))),
            LaunchKind::Backend => {
                if mode == LaunchMode::Resume && agent_ref.is_none() {
                    return Ok(None);
                }
                let params = PrepareParams {
                    agent: name.to_string(),
                    mode,
                    prompt: prompt.map(String::from),
                    agent_ref: agent_ref.map(String::from),
                    settings,
                };
                let params = serde_json::to_value(params).map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?;
                let reply = self.plugin_call(&plugin, protocol::method::AGENT_PREPARE, params, Some(self.call_timeout())).await?;
                let prepared: PrepareResult = serde_json::from_value(reply)
                    .map_err(|e| Error::new(ErrorKind::PluginError, format!("{plugin}: unexpected agent.prepare reply: {e}")))?;
                if prepared.argv.is_empty() {
                    return Err(Error::new(ErrorKind::PluginError, format!("{plugin}: agent.prepare returned an empty command")));
                }
                Ok(Some((prepared.argv, prepared.env)))
            }
        }
    }

    pub async fn start_session(self: &Arc<Self>, params: SessionStartParams) -> Result<Session> {
        let task = self.task(params.task_id)?;
        if task.archived {
            return Err(Error::new(ErrorKind::InvalidParams, format!("task {} is archived", task.id)));
        }
        let (argv, extra_env) = match &params.kind {
            SessionKind::Agent { name } => {
                self.ensure_agent_available(name)?;
                let launched = self.agent_argv(name, LaunchMode::Start, params.prompt.as_deref(), None).await?;
                launched.ok_or_else(|| Error::new(ErrorKind::PluginError, format!("agent {name} has no start command")))?
            }
            SessionKind::Shell => (vec![std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into())], Vec::new()),
            SessionKind::Command { argv } if !argv.is_empty() => (argv.clone(), Vec::new()),
            SessionKind::Command { .. } => {
                return Err(Error::new(ErrorKind::InvalidParams, "command argv must not be empty"));
            }
        };
        let id = self.store().insert_session(task.id, &params.kind, SessionStatus::Working)?;
        if let Err(e) = self.spawn_live(id, &task, argv, extra_env, &params.kind) {
            self.store().set_session_status(id, SessionStatus::Exited)?;
            return Err(e);
        }
        let session = self.session(id)?;
        self.emit(Event::SessionChanged(session.clone()));
        self.touch_task(task.id);
        Ok(session)
    }

    fn spawn_live(
        self: &Arc<Self>,
        id: i64,
        task: &Task,
        argv: Vec<String>,
        extra_env: Vec<(String, String)>,
        kind: &SessionKind,
    ) -> Result<()> {
        let mut policy = Config::load(&self.paths.config())?.env_policy(config::agent_key(kind));
        policy.set.extend(extra_env);
        let (bin_dir, path) = tool_path()?;
        let fixed = [
            ("PATH".to_string(), path),
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
        let waiting_patterns: Arc<[String]> = match kind {
            SessionKind::Agent { name } => self.agent_decl(name).map(|(_, d)| d.waiting_patterns).unwrap_or_default().into(),
            _ => Arc::from(Vec::new()),
        };
        tokio::spawn(status::track(pty.clone(), waiting_patterns, live.status.clone(), live.hooks_active.clone()));

        let daemon = self.clone();
        let task_id = task.id;
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
                daemon.touch_task(task_id);
                if status == SessionStatus::Exited {
                    break;
                }
            }
        });
        Ok(())
    }

    /// Best effort: a failed timestamp update must not break the session it reports on.
    fn touch_task(&self, task_id: i64) {
        let touched = {
            let store = self.store();
            store.touch_task(task_id).and_then(|()| store.task(task_id))
        };
        if let Ok(Some(task)) = touched {
            self.emit(Event::TaskChanged(task));
        }
    }

    fn live(&self, id: i64) -> Result<Arc<LiveSession>> {
        lock(&self.live)
            .get(&id)
            .cloned()
            .ok_or_else(|| Error::new(ErrorKind::NotFound, format!("session {id} is not running")))
    }

    pub fn stats(&self, params: NodeStatsParams) -> NodeStats {
        let sessions: Vec<(i64, u32)> =
            lock(&self.live).iter().filter_map(|(id, live)| Some((*id, live.pty.pid()?))).collect();
        let mut cpu = lock(&self.cpu);
        let mut measure = |pids: &[u32]| {
            let (memory_bytes, cpu_percent) = cpu.measure(pids);
            ProcStats { memory_bytes, cpu_percent }
        };
        let stats = NodeStats {
            daemon: measure(&[std::process::id()]),
            sessions: sessions
                .into_iter()
                .map(|(session_id, pid)| SessionStats { session_id, stats: measure(&proc_stats::tree(pid)) })
                .collect(),
            processes: params.pids.into_iter().map(|pid| PidStats { pid, stats: measure(&[pid]) }).collect(),
        };
        cpu.finish();
        stats
    }

    pub fn session(&self, id: i64) -> Result<Session> {
        self.store().session(id)?.map(|s| s.session).ok_or_else(|| not_found("session", id))
    }

    pub fn sessions(&self, task_id: Option<i64>) -> Result<Vec<Session>> {
        Ok(self.store().sessions(task_id)?.into_iter().map(|s| s.session).collect())
    }

    pub fn agent_config(&self, agent: &str) -> Result<AgentConfig> {
        agent_settings::load(&self.paths, &self.plugin_set().registry, agent)
    }

    pub fn agent_config_raw(&self, agent: &str) -> Result<AgentConfigRaw> {
        agent_settings::load_raw(&self.paths, &self.plugin_set().registry, agent)
    }

    pub fn set_agent_config(&self, agent: &str, config: &AgentConfig) -> Result<()> {
        agent_settings::save(&self.paths, &self.plugin_set().registry, agent, config)
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

    pub async fn recover(self: &Arc<Self>) -> Result<()> {
        let stored = self.store().sessions(None)?;
        for crate::store::StoredSession { session, agent_ref } in stored {
            if session.status == SessionStatus::Exited {
                continue;
            }
            let resumed = match self.task(session.task_id) {
                Ok(task) if !task.archived => {
                    let resume = match &session.kind {
                        SessionKind::Agent { name } => self.agent_argv(name, LaunchMode::Resume, None, agent_ref.as_deref()).await,
                        _ => Ok(None),
                    };
                    match resume {
                        Ok(Some((argv, env))) => {
                            let spawned = self.spawn_live(session.id, &task, argv, env, &session.kind);
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

/// git prints worktree paths with symlinks resolved (`/private/var` on macOS), tasks store them as created.
fn same_path(a: &str, b: &str) -> bool {
    let canonical = |p: &str| std::fs::canonicalize(p).unwrap_or_else(|_| PathBuf::from(p));
    a == b || canonical(a) == canonical(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_env_strips_agent_variables_and_appends_extras_last() {
        std::env::set_var("CLAUDE_ASTERISM_ENV_TEST", "1");
        std::env::set_var("ANTHROPIC_ASTERISM_ENV_TEST", "1");
        let home = tempfile::tempdir().unwrap();
        let paths = Paths { home: home.path().to_path_buf() };
        let env = plugin_env(&paths, &[("EXTRA".into(), "1".into())]).unwrap();
        std::env::remove_var("CLAUDE_ASTERISM_ENV_TEST");
        std::env::remove_var("ANTHROPIC_ASTERISM_ENV_TEST");

        let get = |key: &str| env.iter().filter(|(k, _)| k == key).map(|(_, v)| v.clone()).collect::<Vec<_>>();
        assert!(get("CLAUDE_ASTERISM_ENV_TEST").is_empty() && get("ANTHROPIC_ASTERISM_ENV_TEST").is_empty());
        let path = get("PATH");
        assert_eq!(path.len(), 1);
        let bin_dir = tool_path().unwrap().0;
        assert_eq!(std::env::split_paths(&path[0]).next().unwrap(), bin_dir);
        assert_eq!(get("ASTERISM_HOME"), [paths.home.display().to_string()]);
        assert_eq!(get("ASTERISM_SOCKET"), [paths.socket().display().to_string()]);
        assert!(get("ASTERISM_CLI")[0].ends_with("/asterism"));
        let position = |key: &str| env.iter().position(|(k, _)| k == key).unwrap();
        assert_eq!(get("EXTRA"), ["1"]);
        assert!(position("EXTRA") > position("ASTERISM_CLI"));
    }

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

// A status error must surface; only a signed-out forge falls back to anonymous HTTPS.
fn forge_clone_wanted(status: Result<ForgeStatus>) -> Result<bool> {
    status.map(|s| s.authenticated)
}

#[cfg(test)]
mod forge_clone_tests {
    use super::*;

    #[test]
    fn only_a_signed_out_status_falls_back() {
        assert!(!forge_clone_wanted(Ok(ForgeStatus::default())).unwrap());
        assert!(forge_clone_wanted(Ok(ForgeStatus { authenticated: true, ..Default::default() })).unwrap());
        assert_eq!(forge_clone_wanted(Err(Error::new(ErrorKind::Timeout, "slow"))).unwrap_err().kind, ErrorKind::Timeout);
    }
}
