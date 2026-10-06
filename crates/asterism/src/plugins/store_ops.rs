use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use asterism_proto::rpc::ErrorKind;

use super::catalog::{self, official_source, StoreConfig, StoreIndex, StoresFile};
use super::manifest::is_slug;
use super::source;
use crate::error::{Error, Result};
use crate::git::GitEnv;
use crate::node_settings::expand_home;
use crate::paths::Paths;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn invalid(message: String) -> Error {
    Error::new(ErrorKind::InvalidParams, message)
}

/// Absolute paths (or `~/…`) are local stores, read in place; anything else is a git URL.
pub fn is_local(source: &str) -> bool {
    source.starts_with('/') || source.starts_with('~')
}

pub fn store_dir(paths: &Paths, store: &StoreConfig) -> PathBuf {
    if is_local(&store.source) {
        PathBuf::from(&store.source)
    } else {
        paths.plugin_stores_dir().join(&store.name)
    }
}

pub fn load(paths: &Paths) -> Result<StoresFile> {
    let file = catalog::load_stores(&paths.plugin_stores_file()).map_err(invalid)?;
    if let Some(store) = file.stores.iter().find(|s| !is_slug(&s.name)) {
        return Err(invalid(format!("stores.toml: invalid store name {:?}", store.name)));
    }
    Ok(file)
}

pub fn find_store<'a>(file: &'a StoresFile, name: &str) -> Result<&'a StoreConfig> {
    file.stores.iter().find(|s| s.name == name).ok_or_else(|| Error::new(ErrorKind::NotFound, format!("no store named {name}")))
}

pub fn add_store(paths: &Paths, source: &str, env: &GitEnv) -> Result<StoreConfig> {
    let mut file = load(paths)?;
    let official = official_source().as_deref() == Some(source);
    let (source, index, checkout) = if is_local(source) {
        let dir = expand_home(source)?.canonicalize().map_err(|e| invalid(format!("{source}: {e}")))?;
        (dir.display().to_string(), source::read_index(&dir)?, None)
    } else {
        let temp = paths.plugin_stores_dir().join(format!(".adding-{}-{}", std::process::id(), TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(paths.plugin_stores_dir())?;
        let read = source::clone_store(source, &temp, env).and_then(|()| source::read_index(&temp));
        match read {
            Ok(index) => (source.to_string(), index, Some(temp)),
            Err(e) => {
                let _ = std::fs::remove_dir_all(&temp);
                return Err(e);
            }
        }
    };
    if file.stores.iter().any(|s| s.name == index.name || s.source == source) {
        if let Some(temp) = &checkout {
            let _ = std::fs::remove_dir_all(temp);
        }
        return Err(invalid(format!("a store named {} (or with this source) is already added", index.name)));
    }
    let store = StoreConfig { name: index.name, source, official };
    if let Some(temp) = checkout {
        let target = store_dir(paths, &store);
        let _ = std::fs::remove_dir_all(&target);
        if let Err(e) = std::fs::rename(&temp, &target) {
            let _ = std::fs::remove_dir_all(&temp);
            return Err(e.into());
        }
    }
    file.stores.push(store.clone());
    catalog::save_stores(&paths.plugin_stores_file(), &file)?;
    Ok(store)
}

/// Clones a missing checkout, refreshes an existing one, and returns the index.
pub fn sync_store(paths: &Paths, store: &StoreConfig, env: &GitEnv) -> Result<StoreIndex> {
    let dir = store_dir(paths, store);
    if !is_local(&store.source) {
        if dir.join(".git").exists() {
            source::refresh_store(&dir, env)?;
            if let Err(e) = validate_checkout(&dir, store) {
                let _ = source::restore_store(&dir, env);
                return Err(e);
            }
        } else {
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(paths.plugin_stores_dir())?;
            source::clone_store(&store.source, &dir, env)?;
        }
    }
    validate_checkout(&dir, store)
}

fn validate_checkout(dir: &std::path::Path, store: &StoreConfig) -> Result<StoreIndex> {
    let index = source::read_index(dir)?;
    if index.name != store.name {
        return Err(invalid(format!("store.json of {} now names {:?}; remove and add the store again", store.name, index.name)));
    }
    Ok(index)
}

pub fn remove_store(paths: &Paths, name: &str) -> Result<StoreConfig> {
    let mut file = load(paths)?;
    let store = find_store(&file, name)?.clone();
    file.stores.retain(|s| s.name != name);
    catalog::save_stores(&paths.plugin_stores_file(), &file)?;
    if !is_local(&store.source) {
        let _ = std::fs::remove_dir_all(store_dir(paths, &store));
    }
    Ok(store)
}

/// Indexes of every store that has a readable checkout; the rest are skipped.
pub fn indexes(paths: &Paths, file: &StoresFile) -> Vec<(String, StoreIndex)> {
    file.stores
        .iter()
        .filter_map(|s| Some((s.name.clone(), source::read_index(&store_dir(paths, s)).ok()?)))
        .collect()
}
