pub mod catalog;
pub mod manifest;
pub mod process;
pub mod registry;
pub mod settings;

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crate::paths::Paths;
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
    backends: HashMap<String, Arc<Backend>>,
}

impl PluginSet {
    pub fn load(paths: &Paths, builtin_dir: &Path, runtime: &Runtime, host: &HostFn) -> Self {
        let links = registry::load_links(&paths.plugin_links()).unwrap_or_else(|e| {
            eprintln!("asterismd: ignoring {}: {e}", paths.plugin_links().display());
            BTreeMap::new()
        });
        let registry = Registry::discover(&Sources { builtin_dir: builtin_dir.to_path_buf(), links });
        let mut backends = HashMap::new();
        for plugin in registry.plugins().iter().filter(|p| p.is_ok()) {
            let (Some(manifest), Some(argv)) = (plugin.manifest.as_ref(), plugin.backend_command()) else { continue };
            let settings = settings::resolved(paths, &plugin.name, &manifest.settings).unwrap_or_else(|e| {
                eprintln!("asterismd: plugin {}: ignoring settings: {}", plugin.name, e.message);
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
            backends.insert(plugin.name.clone(), Backend::new(config, host.clone(), settings));
        }
        Self { registry, backends }
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
