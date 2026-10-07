pub mod catalog;
pub mod install;
pub mod manifest;
pub mod process;
pub mod registry;
pub mod settings;
pub mod source;
pub mod store_ops;

/// Serialises every store and install operation; the files they touch are read-modify-write.
pub static STORE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crate::paths::Paths;
use catalog::InstalledFile;
use process::{Backend, BackendConfig, HostFn};
use registry::{Registry, Sources};

/// How plugin backends are started: their base environment and timing.
pub struct Runtime {
    pub env: Vec<(String, String)>,
    pub call_timeout: Duration,
    pub idle: Duration,
}

/// One discovery result and a backend per runnable plugin; replaced wholesale on reload.
pub struct PluginSet {
    pub registry: Registry,
    /// What `installed.toml` said when this set was loaded.
    pub installed: InstalledFile,
    backends: HashMap<String, Arc<Backend>>,
}

impl PluginSet {
    pub fn load(paths: &Paths, builtin_dir: &Path, runtime: &Runtime, host: &HostFn) -> Self {
        let links = registry::load_links(&paths.plugin_links()).unwrap_or_else(|e| {
            eprintln!(
                "asterismd: ignoring {}: {e}",
                paths.plugin_links().display()
            );
            BTreeMap::new()
        });
        let installed =
            catalog::load_installed(&paths.plugin_installed_file()).unwrap_or_else(|e| {
                eprintln!("asterismd: ignoring {e}");
                InstalledFile::default()
            });
        let installed_dirs = installed
            .plugins
            .iter()
            .filter(|(name, entry)| {
                let ok = manifest::is_version(&entry.version);
                if !ok {
                    eprintln!(
                        "asterismd: ignoring installed plugin {name}: invalid version {:?}",
                        entry.version
                    );
                }
                ok
            })
            .map(|(name, entry)| {
                (
                    name.clone(),
                    paths.plugins_installed().join(name).join(&entry.version),
                )
            })
            .collect();
        let registry = Registry::discover(&Sources {
            builtin_dir: builtin_dir.to_path_buf(),
            links,
            installed: installed_dirs,
            disabled: installed.disabled.clone(),
        });
        let mut backends = HashMap::new();
        for plugin in registry.plugins().iter().filter(|p| p.is_ok()) {
            let (Some(manifest), Some(argv)) = (plugin.manifest.as_ref(), plugin.backend_command())
            else {
                continue;
            };
            let settings = settings::resolved(paths, &plugin.name, &manifest.settings)
                .unwrap_or_else(|e| {
                    eprintln!(
                        "asterismd: plugin {}: ignoring settings: {}",
                        plugin.name, e.message
                    );
                    Default::default()
                });
            let mut env = runtime.env.clone();
            env.push(("ASTERISM_PLUGIN_BIN".into(), argv[0].clone()));
            let config = BackendConfig {
                plugin: plugin.name.clone(),
                argv,
                dir: plugin.dir.clone(),
                data_dir: paths.plugin_data(&plugin.name),
                env,
                capabilities: manifest.backend_capabilities(),
                call_timeout: runtime.call_timeout,
                idle: runtime.idle,
            };
            backends.insert(
                plugin.name.clone(),
                Backend::new(config, host.clone(), settings),
            );
        }
        Self {
            registry,
            installed,
            backends,
        }
    }

    pub fn backend(&self, plugin: &str) -> Option<&Arc<Backend>> {
        self.backends.get(plugin)
    }

    pub async fn stop_all(&self) {
        for backend in self.backends.values() {
            backend.stop().await;
        }
    }
}
