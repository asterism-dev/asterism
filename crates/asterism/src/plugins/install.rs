use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use asterism_proto::rpc::ErrorKind;

use super::catalog::{self, EntrySource, IndexEntry, InstalledEntry, InstalledFile, StoreConfig};
use super::manifest::{self, is_slug, is_version, Manifest};
use super::{source, store_ops};
use crate::error::{Error, Result};
use crate::git::GitEnv;
use crate::paths::Paths;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub struct Resolved {
    pub dir: PathBuf,
    pub manifest: Manifest,
    pub git_ref: Option<String>,
}

fn invalid(message: String) -> Error {
    Error::new(ErrorKind::InvalidParams, message)
}

fn not_found(message: String) -> Error {
    Error::new(ErrorKind::NotFound, message)
}

fn check_name(name: &str) -> Result<()> {
    if is_slug(name) { Ok(()) } else { Err(invalid(format!("invalid plugin name {name:?}"))) }
}

fn check_version(version: &str) -> Result<()> {
    if is_version(version) { Ok(()) } else { Err(invalid(format!("invalid version {version:?} in installed.toml"))) }
}

pub fn load(paths: &Paths) -> Result<InstalledFile> {
    catalog::load_installed(&paths.plugin_installed_file()).map_err(invalid)
}

fn save(paths: &Paths, file: &InstalledFile) -> Result<()> {
    catalog::save_installed(&paths.plugin_installed_file(), file)
}

pub fn find_entry(paths: &Paths, store: &str, name: &str) -> Result<(StoreConfig, IndexEntry)> {
    let file = store_ops::load(paths)?;
    let store = store_ops::find_store(&file, store)?.clone();
    let index = source::read_index(&store_ops::store_dir(paths, &store))?;
    let entry = index.plugins.into_iter().find(|e| e.name == name).ok_or_else(|| not_found(format!("store {} has no plugin {name}", store.name)))?;
    Ok((store, entry))
}

pub fn resolve(paths: &Paths, store: &StoreConfig, entry: &IndexEntry, env: &GitEnv) -> Result<Resolved> {
    let (dir, git_ref) = match entry.source() {
        EntrySource::Local { path } => (source::plugin_dir(&store_ops::store_dir(paths, store), path)?, None),
        EntrySource::Git { url, git_ref, path } => {
            let checkout = source::checkout_git(&paths.plugin_cache(), url, git_ref, env)?;
            (source::plugin_dir(&checkout, path)?, Some(git_ref.to_string()))
        }
    };
    let text = std::fs::read_to_string(dir.join("plugin.toml")).map_err(|e| invalid(format!("{}/{}: no plugin.toml: {e}", store.name, entry.name)))?;
    let manifest = manifest::parse(&text).map_err(|e| invalid(format!("{}/{}: {e}", store.name, entry.name)))?;
    if manifest.name != entry.name {
        return Err(invalid(format!("store entry {} points at plugin {}", entry.name, manifest.name)));
    }
    Ok(Resolved { dir, manifest, git_ref })
}

/// Copies into a temp dir first and renames, so a failed copy never leaves a half-installed version.
pub fn install_files(paths: &Paths, resolved: &Resolved) -> Result<()> {
    let base = paths.plugins_installed().join(&resolved.manifest.name);
    std::fs::create_dir_all(&base)?;
    let temp = base.join(format!(".tmp-{}-{}", std::process::id(), TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)));
    if let Err(e) = source::copy_tree(&resolved.dir, &temp) {
        let _ = std::fs::remove_dir_all(&temp);
        return Err(e);
    }
    let target = base.join(&resolved.manifest.version);
    let _ = std::fs::remove_dir_all(&target);
    std::fs::rename(&temp, &target)?;
    Ok(())
}

/// The old version becomes `previous`; the version before that is deleted.
pub fn record(paths: &Paths, store: &str, resolved: &Resolved) -> Result<InstalledEntry> {
    let mut file = load(paths)?;
    let name = &resolved.manifest.name;
    let version = resolved.manifest.version.clone();
    let old = file.plugins.get(name).cloned();
    if let Some(old) = &old {
        check_version(&old.version)?;
        old.previous.as_deref().map(check_version).transpose()?;
    }
    let previous = match &old {
        Some(old) if old.version != version => Some(old.version.clone()),
        Some(old) => old.previous.clone(),
        None => None,
    };
    if let Some(stale) = old.and_then(|o| o.previous).filter(|p| Some(p) != previous.as_ref() && *p != version) {
        let _ = std::fs::remove_dir_all(paths.plugins_installed().join(name).join(stale));
    }
    let entry = InstalledEntry { version, store: store.to_string(), git_ref: resolved.git_ref.clone(), previous };
    file.plugins.insert(name.clone(), entry.clone());
    save(paths, &file)?;
    Ok(entry)
}

pub fn rollback(paths: &Paths, name: &str) -> Result<InstalledEntry> {
    check_name(name)?;
    let mut file = load(paths)?;
    let entry = file.plugins.get_mut(name).ok_or_else(|| not_found(format!("plugin {name} is not installed from a store")))?;
    let previous = entry.previous.clone().ok_or_else(|| invalid(format!("plugin {name} has no previous version")))?;
    check_version(&previous)?;
    if !paths.plugins_installed().join(name).join(&previous).is_dir() {
        return Err(invalid(format!("version {previous} of {name} is no longer on disk")));
    }
    entry.previous = Some(std::mem::replace(&mut entry.version, previous));
    // The rolled-back-from version may have a different ref; the next refresh recomputes updates from the version.
    entry.git_ref = None;
    let entry = entry.clone();
    save(paths, &file)?;
    Ok(entry)
}

/// Removes every installed version; `data/<name>/` stays so settings survive a reinstall.
pub fn uninstall(paths: &Paths, name: &str) -> Result<()> {
    check_name(name)?;
    let mut file = load(paths)?;
    if file.plugins.remove(name).is_none() {
        return Err(not_found(format!("plugin {name} is not installed from a store")));
    }
    file.disabled.remove(name);
    save(paths, &file)?;
    let _ = std::fs::remove_dir_all(paths.plugins_installed().join(name));
    Ok(())
}

pub fn set_enabled(paths: &Paths, name: &str, enabled: bool) -> Result<()> {
    check_name(name)?;
    let mut file = load(paths)?;
    if enabled {
        file.disabled.remove(name);
    } else {
        file.disabled.insert(name.to_string());
    }
    save(paths, &file)
}

pub fn same_permissions(a: &[String], b: &[String]) -> bool {
    a.iter().collect::<BTreeSet<_>>() == b.iter().collect::<BTreeSet<_>>()
}

pub fn adds_permissions(old: &[String], new: &[String]) -> bool {
    new.iter().any(|p| !old.contains(p))
}
