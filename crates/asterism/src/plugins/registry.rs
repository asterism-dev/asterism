use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use asterism_proto::types::{Capability, CapabilityKind, PluginOrigin};
use serde::{Deserialize, Serialize};

use super::manifest::{self, AgentDecl, CommandDecl, ForgeDecl, Manifest, TaskSourceDecl};
use super::ui;
use crate::agent_settings::write_atomic;

/// Manifests of the plugins shipped with asterism; their backends sit next to `asterismd`.
pub const BUILTIN: &[(&str, &str)] = &[
    ("claude", include_str!("../../plugins/claude/plugin.toml")),
    ("github", include_str!("../../plugins/github/plugin.toml")),
    ("linear", include_str!("../../plugins/linear/plugin.toml")),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Ok,
    Broken(String),
    Disabled,
}

#[derive(Debug)]
pub struct Plugin {
    pub name: String,
    pub origin: PluginOrigin,
    pub dir: PathBuf,
    pub manifest: Option<Manifest>,
    pub status: Status,
}

impl Plugin {
    pub fn is_ok(&self) -> bool {
        self.status == Status::Ok
    }

    /// Programs with a `/` are relative to the plugin dir; bare names are looked up on PATH at spawn.
    pub fn backend_command(&self) -> Option<Vec<String>> {
        let mut argv = self.manifest.as_ref()?.backend.as_ref()?.command.clone();
        if argv[0].contains('/') {
            argv[0] = self.dir.join(&argv[0]).display().to_string();
        }
        Some(argv)
    }

    pub fn capabilities(&self) -> Vec<Capability> {
        self.manifest
            .as_ref()
            .map(Manifest::capabilities)
            .unwrap_or_default()
    }
}

#[derive(Debug, Default)]
pub struct Sources {
    pub builtin_dir: PathBuf,
    pub links: BTreeMap<String, PathBuf>,
    /// Plugin name → directory of its current installed version.
    pub installed: BTreeMap<String, PathBuf>,
    pub disabled: BTreeSet<String>,
}

#[derive(Debug, Default)]
pub struct Registry {
    plugins: Vec<Arc<Plugin>>,
}

fn candidate(
    expected: &str,
    origin: PluginOrigin,
    dir: PathBuf,
    text: io::Result<String>,
) -> Plugin {
    let parsed = text
        .map_err(|e| format!("cannot read {}: {e}", dir.join("plugin.toml").display()))
        .and_then(|text| manifest::parse(&text));
    let (manifest, status) = match parsed {
        Ok(m) if m.name != expected => {
            let reason = format!("plugin.toml names {:?}, expected {expected:?}", m.name);
            (Some(m), Status::Broken(reason))
        }
        Ok(m) => (Some(m), Status::Ok),
        Err(reason) => (None, Status::Broken(reason)),
    };
    Plugin {
        name: expected.to_string(),
        origin,
        dir,
        manifest,
        status,
    }
}

impl Registry {
    /// Linked plugins come first, then installed, then built-in; the first plugin with a name wins, and the first to claim a capability keeps it.
    pub fn discover(sources: &Sources) -> Self {
        let read = |origin: PluginOrigin| {
            move |(name, dir): (&String, &PathBuf)| {
                candidate(
                    name,
                    origin,
                    dir.clone(),
                    std::fs::read_to_string(dir.join("plugin.toml")),
                )
            }
        };
        let linked = sources.links.iter().map(read(PluginOrigin::Linked));
        let installed = sources.installed.iter().map(read(PluginOrigin::Installed));
        let builtin = BUILTIN.iter().map(|(name, text)| {
            candidate(
                name,
                PluginOrigin::Builtin,
                sources.builtin_dir.clone(),
                Ok(text.to_string()),
            )
        });
        let mut plugins: Vec<Arc<Plugin>> = Vec::new();
        let mut claimed: Vec<(CapabilityKind, String, String)> = Vec::new();
        for mut plugin in linked.chain(installed).chain(builtin) {
            if plugins.iter().any(|p| p.name == plugin.name) {
                continue;
            }
            if plugin.is_ok() && sources.disabled.contains(&plugin.name) {
                plugin.status = Status::Disabled;
            }
            if plugin.is_ok() {
                let reason = plugin.manifest.as_ref().and_then(|m| {
                    m.provides
                        .panel
                        .iter()
                        .find(|p| ui::read(&plugin, &p.entry).is_none())
                        .map(|p| format!("panel {}: cannot read {}", p.id, p.entry))
                });
                if let Some(reason) = reason {
                    plugin.status = Status::Broken(reason);
                }
            }
            if plugin.is_ok() {
                let capabilities = plugin.capabilities();
                let clash = capabilities.iter().find_map(|c| {
                    claimed
                        .iter()
                        .find(|(kind, id, _)| *kind == c.kind && *id == c.id)
                        .map(|(_, id, owner)| (id.clone(), owner.clone()))
                });
                match clash {
                    Some((id, owner)) => {
                        plugin.status =
                            Status::Broken(format!("{id:?} is already provided by plugin {owner}"))
                    }
                    None => claimed.extend(
                        capabilities
                            .into_iter()
                            .map(|c| (c.kind, c.id, plugin.name.clone())),
                    ),
                }
            }
            plugins.push(Arc::new(plugin));
        }
        Self { plugins }
    }

    pub fn plugins(&self) -> &[Arc<Plugin>] {
        &self.plugins
    }

    pub fn get(&self, name: &str) -> Option<&Arc<Plugin>> {
        self.plugins.iter().find(|p| p.name == name)
    }

    fn ok(&self) -> impl Iterator<Item = (&Arc<Plugin>, &Manifest)> {
        self.plugins
            .iter()
            .filter(|p| p.is_ok())
            .filter_map(|p| Some((p, p.manifest.as_ref()?)))
    }

    pub fn agents(&self) -> impl Iterator<Item = (&Arc<Plugin>, &AgentDecl)> {
        self.ok()
            .flat_map(|(p, m)| m.provides.agent.iter().map(move |a| (p, a)))
    }

    pub fn forges(&self) -> impl Iterator<Item = (&Arc<Plugin>, &ForgeDecl)> {
        self.ok()
            .flat_map(|(p, m)| m.provides.forge.iter().map(move |f| (p, f)))
    }

    pub fn commands(&self) -> impl Iterator<Item = (&Arc<Plugin>, &CommandDecl)> {
        self.ok()
            .flat_map(|(p, m)| m.provides.command.iter().map(move |c| (p, c)))
    }

    pub fn task_sources(&self) -> impl Iterator<Item = (&Arc<Plugin>, &TaskSourceDecl)> {
        self.ok()
            .flat_map(|(p, m)| m.provides.task_source.iter().map(move |t| (p, t)))
    }

    pub fn agent(&self, id: &str) -> Option<(&Arc<Plugin>, &AgentDecl)> {
        self.agents().find(|(_, a)| a.id == id)
    }

    pub fn forge(&self, id: &str) -> Option<(&Arc<Plugin>, &ForgeDecl)> {
        self.forges().find(|(_, f)| f.id == id)
    }

    pub fn task_source(&self, id: &str) -> Option<(&Arc<Plugin>, &TaskSourceDecl)> {
        self.task_sources().find(|(_, t)| t.id == id)
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LinksFile {
    #[serde(default)]
    links: BTreeMap<String, PathBuf>,
}

pub fn load_links(path: &Path) -> Result<BTreeMap<String, PathBuf>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => toml::from_str::<LinksFile>(&text)
            .map(|f| f.links)
            .map_err(|e| e.to_string()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(e.to_string()),
    }
}

pub fn save_links(path: &Path, links: &BTreeMap<String, PathBuf>) -> crate::error::Result<()> {
    let text = toml::to_string(&LinksFile {
        links: links.clone(),
    })
    .map_err(|e| {
        crate::error::Error::new(asterism_proto::rpc::ErrorKind::Internal, e.to_string())
    })?;
    write_atomic(path, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin_dir(root: &Path, dir: &str, manifest: &str) -> PathBuf {
        let path = root.join(dir);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("plugin.toml"), manifest).unwrap();
        path
    }

    fn forge_manifest(name: &str, forge: &str) -> String {
        format!("name = \"{name}\"\nversion = \"9.0.0\"\nprotocol = 1\n[backend]\ncommand = [\"python3\", \"x.py\"]\n[[provides.forge]]\nid = \"{forge}\"\ndisplay_name = \"X\"\n")
    }

    fn sources(links: &[(&str, PathBuf)]) -> Sources {
        Sources {
            builtin_dir: PathBuf::from("/opt/asterism/bin"),
            links: links
                .iter()
                .map(|(n, p)| (n.to_string(), p.clone()))
                .collect(),
            installed: BTreeMap::new(),
            disabled: std::collections::BTreeSet::new(),
        }
    }

    #[test]
    fn installed_plugins_sit_between_linked_and_builtin() {
        let root = tempfile::tempdir().unwrap();
        let installed = plugin_dir(
            root.path(),
            "installed",
            &forge_manifest("github", "github"),
        );
        let linked = plugin_dir(root.path(), "linked", &forge_manifest("github", "github"));
        let mut s = sources(&[]);
        s.installed.insert("github".into(), installed.clone());
        let registry = Registry::discover(&s);
        assert_eq!(
            (
                registry.get("github").unwrap().origin,
                registry.get("github").unwrap().dir.clone()
            ),
            (PluginOrigin::Installed, installed)
        );
        s.links.insert("github".into(), linked);
        assert_eq!(
            Registry::discover(&s).get("github").unwrap().origin,
            PluginOrigin::Linked
        );
    }

    #[test]
    fn disabled_plugins_stay_listed_but_claim_nothing() {
        let mut s = sources(&[]);
        s.disabled.insert("github".into());
        let registry = Registry::discover(&s);
        assert_eq!(registry.get("github").unwrap().status, Status::Disabled);
        assert!(registry.forge("github").is_none());
        assert!(registry.agent("claude").is_some());
    }

    #[test]
    fn missing_installed_dir_is_broken() {
        let mut s = sources(&[]);
        s.installed.insert(
            "linear".into(),
            PathBuf::from("/definitely/missing/linear/1.0.0"),
        );
        assert!(
            matches!(&Registry::discover(&s).get("linear").unwrap().status, Status::Broken(r) if r.contains("cannot read"))
        );
    }

    #[test]
    fn builtins_are_discovered() {
        let registry = Registry::discover(&sources(&[]));
        let names: Vec<_> = registry
            .plugins()
            .iter()
            .map(|p| (p.name.as_str(), p.is_ok()))
            .collect();
        assert_eq!(
            names,
            [("claude", true), ("github", true), ("linear", true)]
        );
        let (plugin, forge) = registry.forge("github").unwrap();
        assert_eq!(
            (plugin.name.as_str(), forge.hosts.as_slice()),
            ("github", &["github.com".to_string()][..])
        );
        assert_eq!(registry.agent("claude").unwrap().1.binary, "claude");
        assert_eq!(
            registry.task_source("github-issues").unwrap().0.name,
            "github"
        );
        assert_eq!(
            plugin.backend_command().unwrap(),
            ["/opt/asterism/bin/./asterism-plugin-github"]
        );
    }

    #[test]
    fn linked_plugins_override_builtins_of_the_same_name() {
        let root = tempfile::tempdir().unwrap();
        let dir = plugin_dir(root.path(), "gh-dev", &forge_manifest("github", "github"));
        let registry = Registry::discover(&sources(&[("github", dir.clone())]));
        let github = registry.get("github").unwrap();
        assert_eq!(
            (github.origin, github.dir.clone()),
            (PluginOrigin::Linked, dir)
        );
        assert_eq!(registry.plugins().len(), 3);
        assert_eq!(github.backend_command().unwrap(), ["python3", "x.py"]);
    }

    #[test]
    fn duplicate_capabilities_break_the_lower_precedence_plugin() {
        let root = tempfile::tempdir().unwrap();
        let dir = plugin_dir(root.path(), "other", &forge_manifest("other", "github"));
        let registry = Registry::discover(&sources(&[("other", dir)]));
        assert!(registry.get("other").unwrap().is_ok());
        let Status::Broken(reason) = &registry.get("github").unwrap().status else {
            panic!("github should be broken")
        };
        assert!(
            reason.contains("already provided by plugin other"),
            "{reason}"
        );
        assert_eq!(registry.forge("github").unwrap().0.name, "other");
    }

    #[test]
    fn missing_linked_dir_is_broken() {
        let registry = Registry::discover(&sources(&[(
            "ghost",
            PathBuf::from("/definitely/missing/plugin"),
        )]));
        let ghost = registry.get("ghost").unwrap();
        let Status::Broken(reason) = &ghost.status else {
            panic!("ghost should be broken")
        };
        assert!(reason.contains("cannot read"), "{reason}");
        assert!(ghost.manifest.is_none() && ghost.capabilities().is_empty());
    }

    #[test]
    fn linked_name_must_match_the_manifest() {
        let root = tempfile::tempdir().unwrap();
        let dir = plugin_dir(root.path(), "x", &forge_manifest("actual", "x-forge"));
        let registry = Registry::discover(&sources(&[("expected", dir)]));
        assert!(
            matches!(&registry.get("expected").unwrap().status, Status::Broken(r) if r.contains("\"actual\""))
        );
    }

    #[test]
    fn links_roundtrip_and_tolerate_a_missing_file() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("plugins/links.toml");
        assert!(load_links(&path).unwrap().is_empty());
        let links = BTreeMap::from([("echo".to_string(), PathBuf::from("/src/echo"))]);
        save_links(&path, &links).unwrap();
        assert_eq!(load_links(&path).unwrap(), links);
        std::fs::write(&path, "garbage = [").unwrap();
        assert!(load_links(&path).is_err());
    }
}
