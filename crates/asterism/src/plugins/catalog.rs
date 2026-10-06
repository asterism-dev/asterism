use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Component, Path};

use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::CapabilityKind;
use serde::{Deserialize, Serialize};

use crate::agent_settings::write_atomic;
use crate::error::{Error, Result};

pub const OFFICIAL_STORE: &str = "https://github.com/asterism-dev/asterism-plugins.git";
pub const OFFICIAL_NAME: &str = "asterism-dev";
pub const INDEX_FORMAT: u32 = 1;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StoreIndex {
    pub format: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub plugins: Vec<IndexEntry>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct IndexEntry {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub git: Option<String>,
    #[serde(default, rename = "ref")]
    pub git_ref: Option<String>,
}

pub enum EntrySource<'a> {
    Local { path: &'a str },
    Git { url: &'a str, git_ref: &'a str, path: &'a str },
}

impl IndexEntry {
    /// Valid after `parse_index`, which guarantees exactly one of the two shapes.
    pub fn source(&self) -> EntrySource<'_> {
        match (&self.git, &self.git_ref) {
            (Some(url), Some(git_ref)) => EntrySource::Git { url, git_ref, path: self.path.as_deref().unwrap_or(".") },
            _ => EntrySource::Local { path: self.path.as_deref().unwrap_or(".") },
        }
    }
}

fn is_slug(s: &str) -> bool {
    !s.is_empty() && !s.starts_with('-') && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A relative path that stays inside its root: no absolute prefix and no `..`.
pub fn safe_relative(path: &str) -> bool {
    !path.is_empty() && Path::new(path).components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

pub fn parse_index(text: &str) -> std::result::Result<StoreIndex, String> {
    let index: StoreIndex = serde_json::from_str(text).map_err(|e| format!("invalid store.json: {e}"))?;
    if index.format != INDEX_FORMAT {
        return Err(format!("unsupported store.json format {} (this asterism reads {INDEX_FORMAT})", index.format));
    }
    if !is_slug(&index.name) {
        return Err(format!("invalid store name {:?}: use a-z, 0-9 and '-'", index.name));
    }
    let mut seen = BTreeSet::new();
    for entry in &index.plugins {
        if !is_slug(&entry.name) {
            return Err(format!("invalid plugin name {:?}", entry.name));
        }
        if !seen.insert(&entry.name) {
            return Err(format!("duplicate plugin {:?}", entry.name));
        }
        match (entry.git.is_some(), entry.git_ref.is_some()) {
            (true, false) => return Err(format!("{}: a git entry needs a ref", entry.name)),
            (false, true) => return Err(format!("{}: ref is only valid with git", entry.name)),
            (false, false) if entry.path.is_none() => return Err(format!("{}: needs either a path or a git source", entry.name)),
            _ => {}
        }
        if let Some(path) = &entry.path {
            if !safe_relative(path) {
                return Err(format!("{}: path {path:?} must be relative and stay inside the repository", entry.name));
            }
        }
    }
    Ok(index)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StoreConfig {
    pub name: String,
    /// A git URL, or an absolute directory that is read in place.
    pub source: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub official: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StoresFile {
    #[serde(default)]
    pub auto_update: bool,
    #[serde(default, rename = "store")]
    pub stores: Vec<StoreConfig>,
}

/// `ASTERISM_OFFICIAL_STORE` overrides the official store; an empty value disables it.
pub fn official_source() -> Option<String> {
    match std::env::var("ASTERISM_OFFICIAL_STORE") {
        Ok(source) if source.is_empty() => None,
        Ok(source) => Some(source),
        Err(_) => Some(OFFICIAL_STORE.to_string()),
    }
}

pub fn load_stores(path: &Path) -> std::result::Result<StoresFile, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(StoresFile {
            auto_update: false,
            stores: official_source()
                .map(|source| StoreConfig { name: OFFICIAL_NAME.into(), source, official: true })
                .into_iter()
                .collect(),
        }),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

pub fn save_stores(path: &Path, file: &StoresFile) -> Result<()> {
    let text = toml::to_string(file).map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?;
    write_atomic(path, &text)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InstalledEntry {
    pub version: String,
    pub store: String,
    /// The pinned ref for git entries; compared with the index to detect updates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InstalledFile {
    #[serde(default)]
    pub plugins: BTreeMap<String, InstalledEntry>,
    /// Disabled plugin names of any origin.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub disabled: BTreeSet<String>,
}

pub fn load_installed(path: &Path) -> std::result::Result<InstalledFile, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(InstalledFile::default()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

pub fn save_installed(path: &Path, file: &InstalledFile) -> Result<()> {
    let text = toml::to_string(file).map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?;
    write_atomic(path, &text)
}

pub fn capability_tag(kind: CapabilityKind) -> &'static str {
    match kind {
        CapabilityKind::Forge => "forge",
        CapabilityKind::Agent => "agent",
        CapabilityKind::Command => "command",
        CapabilityKind::TaskSource => "task_source",
    }
}

/// Case-insensitive substring search; name matches come first, then description/tag matches.
pub fn search<'a>(
    indexes: &'a [(String, StoreIndex)],
    query: Option<&str>,
    capability: Option<CapabilityKind>,
    store: Option<&str>,
) -> Vec<(&'a str, &'a IndexEntry)> {
    let needle = query.map(str::trim).filter(|q| !q.is_empty()).map(str::to_lowercase);
    let mut by_name = Vec::new();
    let mut by_text = Vec::new();
    for (store_name, index) in indexes.iter().filter(|(name, _)| store.is_none_or(|s| s == name)) {
        for entry in &index.plugins {
            if capability.is_some_and(|kind| !entry.tags.iter().any(|t| t == capability_tag(kind))) {
                continue;
            }
            let Some(needle) = &needle else {
                by_name.push((store_name.as_str(), entry));
                continue;
            };
            if entry.name.to_lowercase().contains(needle) {
                by_name.push((store_name.as_str(), entry));
            } else if entry.description.to_lowercase().contains(needle) || entry.tags.iter().any(|t| t.to_lowercase().contains(needle)) {
                by_text.push((store_name.as_str(), entry));
            }
        }
    }
    by_name.extend(by_text);
    by_name
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDEX: &str = r#"{
      "format": 1, "name": "acme", "description": "Acme plugins",
      "plugins": [
        { "name": "gitlab", "path": "plugins/gitlab", "description": "GitLab forge", "tags": ["forge"] },
        { "name": "linear", "git": "https://example.com/linear.git", "ref": "v0.3.0", "description": "Issues", "tags": ["task_source"] },
        { "name": "lab-tools", "path": ".", "description": "Helpers for gitlab users", "tags": ["command"] }
      ]
    }"#;

    #[test]
    fn parses_path_and_git_entries() {
        let index = parse_index(INDEX).unwrap();
        assert_eq!(index.name, "acme");
        assert!(matches!(index.plugins[0].source(), EntrySource::Local { path: "plugins/gitlab" }));
        assert!(matches!(index.plugins[1].source(), EntrySource::Git { url: "https://example.com/linear.git", git_ref: "v0.3.0", path: "." }));
    }

    #[test]
    fn rejects_bad_indexes() {
        let replace = |from: &str, to: &str| parse_index(&INDEX.replace(from, to));
        assert!(replace("\"format\": 1", "\"format\": 2").unwrap_err().contains("format"));
        assert!(replace("\"name\": \"acme\"", "\"name\": \"Acme Inc\"").unwrap_err().contains("store name"));
        assert!(replace("\"ref\": \"v0.3.0\", ", "").unwrap_err().contains("ref"));
        assert!(replace("\"path\": \"plugins/gitlab\"", "\"path\": \"../gitlab\"").unwrap_err().contains("path"));
        assert!(replace("\"path\": \"plugins/gitlab\"", "\"path\": \"/abs\"").unwrap_err().contains("path"));
        assert!(replace("\"name\": \"linear\"", "\"name\": \"gitlab\"").unwrap_err().contains("duplicate"));
        let neither = INDEX.replace("\"path\": \"plugins/gitlab\", ", "");
        assert!(parse_index(&neither).unwrap_err().contains("either"));
        assert!(parse_index("not json").is_err());
    }

    #[test]
    fn search_puts_name_matches_first_and_filters() {
        let indexes = vec![("acme".to_string(), parse_index(INDEX).unwrap())];
        let names = |hits: Vec<(&str, &IndexEntry)>| hits.into_iter().map(|(_, e)| e.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(search(&indexes, Some("GITLAB"), None, None)), ["gitlab", "lab-tools"]);
        assert_eq!(names(search(&indexes, None, Some(CapabilityKind::TaskSource), None)), ["linear"]);
        assert_eq!(names(search(&indexes, Some("issues"), None, None)), ["linear"]);
        assert!(search(&indexes, None, None, Some("other")).is_empty());
        assert_eq!(search(&indexes, None, None, None).len(), 3);
    }

    #[test]
    fn missing_stores_file_means_the_official_store() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stores.toml");
        let file = load_stores(&path).unwrap();
        if official_source().is_some() {
            assert_eq!((file.stores[0].name.as_str(), file.stores[0].official), (OFFICIAL_NAME, true));
        }
        let custom = StoresFile {
            auto_update: true,
            stores: vec![StoreConfig { name: "acme".into(), source: "/srv/acme".into(), official: false }],
        };
        save_stores(&path, &custom).unwrap();
        assert_eq!(load_stores(&path).unwrap(), custom);
        std::fs::write(&path, "store = 3").unwrap();
        assert!(load_stores(&path).is_err());
    }

    #[test]
    fn installed_file_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("installed.toml");
        assert_eq!(load_installed(&path).unwrap(), InstalledFile::default());
        let mut file = InstalledFile::default();
        file.plugins.insert(
            "linear".into(),
            InstalledEntry { version: "0.3.0".into(), store: "acme".into(), git_ref: Some("v0.3.0".into()), previous: Some("0.2.0".into()) },
        );
        file.disabled.insert("github".into());
        save_installed(&path, &file).unwrap();
        assert_eq!(load_installed(&path).unwrap(), file);
    }
}
