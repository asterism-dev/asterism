use std::collections::BTreeMap;
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use asterism_proto::paths::Paths;
use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::{PluginSettings, SettingSpec, SettingType};
use serde_json::{Map, Value};

use crate::agent_settings::{write_atomic, SAVE_LOCK};
use crate::config::Config;
use crate::error::{Error, Result};

type Secrets = BTreeMap<String, BTreeMap<String, String>>;

fn invalid(message: String) -> Error {
    Error::new(ErrorKind::InvalidParams, message)
}

fn load_secrets(paths: &Paths) -> Result<Secrets> {
    let path = paths.secrets();
    match std::fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text).map_err(|e| invalid(format!("{}: {e}", path.display()))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Secrets::new()),
        Err(e) => Err(e.into()),
    }
}

/// Atomic like `write_atomic`, but the file is never readable by others, not even briefly.
pub fn write_private(path: &Path, contents: &str) -> Result<()> {
    let dir = path.parent().ok_or_else(|| Error::new(ErrorKind::Internal, "path has no parent"))?;
    std::fs::create_dir_all(dir)?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let temp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let result = (|| -> io::Result<()> {
        let mut file = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&temp)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    Ok(result?)
}

/// Every value the plugin receives: stored values and secrets, defaults for the rest.
pub fn resolved(paths: &Paths, plugin: &str, schema: &[SettingSpec]) -> Result<Map<String, Value>> {
    let stored = Config::load(&paths.config())?.plugins.remove(plugin).unwrap_or_default();
    let secrets = load_secrets(paths)?.remove(plugin).unwrap_or_default();
    let mut values = Map::new();
    for spec in schema {
        let value = match spec.kind {
            SettingType::Secret => secrets.get(&spec.key).map(|s| Value::String(s.clone())),
            _ => stored.get(&spec.key).and_then(|v| serde_json::to_value(v).ok()),
        };
        if let Some(value) = value.or_else(|| spec.default.clone()) {
            values.insert(spec.key.clone(), value);
        }
    }
    Ok(values)
}

pub fn view(paths: &Paths, plugin: &str, schema: &[SettingSpec]) -> Result<PluginSettings> {
    let is_secret = |key: &str| schema.iter().any(|s| s.key == key && s.kind == SettingType::Secret);
    let mut settings = PluginSettings { schema: schema.to_vec(), values: BTreeMap::new(), secrets_set: Vec::new() };
    for (key, value) in resolved(paths, plugin, schema)? {
        if is_secret(&key) {
            settings.secrets_set.push(key);
        } else {
            settings.values.insert(key, value);
        }
    }
    Ok(settings)
}

/// Titles of required settings without a value.
pub fn missing(paths: &Paths, plugin: &str, schema: &[SettingSpec]) -> Result<Vec<String>> {
    let values = resolved(paths, plugin, schema)?;
    Ok(schema.iter().filter(|s| s.required && !values.contains_key(&s.key)).map(|s| s.title.clone()).collect())
}

fn check(spec: &SettingSpec, value: &Value) -> Result<()> {
    let valid = match (spec.kind, value) {
        (_, Value::Null) => true,
        (SettingType::String | SettingType::Secret, Value::String(_)) => true,
        (SettingType::Bool, Value::Bool(_)) => true,
        (SettingType::Number, Value::Number(_)) => true,
        (SettingType::Enum, Value::String(s)) => spec.options.contains(s),
        _ => false,
    };
    if valid {
        Ok(())
    } else if spec.kind == SettingType::Enum {
        Err(invalid(format!("{}: expected one of {}", spec.title, spec.options.join(", "))))
    } else {
        let expected = match spec.kind {
            SettingType::Bool => "true or false",
            SettingType::Number => "a number",
            _ => "text",
        };
        Err(invalid(format!("{}: expected {expected}", spec.title)))
    }
}

/// Applies updates; `null` restores the default or clears a secret.
pub fn save(paths: &Paths, plugin: &str, schema: &[SettingSpec], updates: &BTreeMap<String, Value>) -> Result<()> {
    let mut checked = Vec::new();
    for (key, value) in updates {
        let spec = schema.iter().find(|s| &s.key == key).ok_or_else(|| invalid(format!("unknown setting {key:?}")))?;
        check(spec, value)?;
        checked.push((spec, value));
    }
    let _guard = crate::lock(&SAVE_LOCK);
    let mut config = Config::load(&paths.config())?;
    let mut secrets = load_secrets(paths)?;
    let stored = config.plugins.entry(plugin.to_string()).or_default();
    let stored_secrets = secrets.entry(plugin.to_string()).or_default();
    for (spec, value) in checked {
        match (spec.kind, value) {
            (_, Value::Null) => {
                stored.remove(&spec.key);
                stored_secrets.remove(&spec.key);
            }
            (SettingType::Secret, Value::String(secret)) => {
                stored_secrets.insert(spec.key.clone(), secret.clone());
            }
            (_, value) => {
                let value = toml::Value::try_from(value).map_err(|e| invalid(format!("{}: {e}", spec.title)))?;
                stored.insert(spec.key.clone(), value);
            }
        }
    }
    config.plugins.retain(|_, values| !values.is_empty());
    secrets.retain(|_, values| !values.is_empty());
    let text = toml::to_string(&config).map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?;
    write_atomic(&paths.config(), &text)?;
    let text = toml::to_string(&secrets).map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?;
    write_private(&paths.secrets(), &text)
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use serde_json::json;

    fn schema() -> Vec<SettingSpec> {
        serde_json::from_value(json!([
            {"key": "token", "title": "Token", "type": "secret", "required": true},
            {"key": "region", "title": "Region", "type": "enum", "options": ["eu", "us"], "default": "eu"},
            {"key": "verbose", "title": "Verbose", "type": "bool"},
            {"key": "limit", "title": "Limit", "type": "number"}
        ]))
        .unwrap()
    }

    fn temp_paths() -> (tempfile::TempDir, Paths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths { home: dir.path().join("h") };
        paths.ensure_dirs().unwrap();
        (dir, paths)
    }

    fn updates(value: serde_json::Value) -> BTreeMap<String, Value> {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn defaults_and_missing_required_values() {
        let (_dir, paths) = temp_paths();
        let values = resolved(&paths, "echo", &schema()).unwrap();
        assert_eq!(values.get("region"), Some(&json!("eu")));
        assert_eq!(missing(&paths, "echo", &schema()).unwrap(), ["Token"]);
    }

    #[test]
    fn secrets_are_masked_and_private() {
        let (_dir, paths) = temp_paths();
        save(&paths, "echo", &schema(), &updates(json!({"token": "s3cret", "region": "us", "verbose": true, "limit": 5}))).unwrap();

        assert_eq!(resolved(&paths, "echo", &schema()).unwrap().get("token"), Some(&json!("s3cret")));
        let shown = view(&paths, "echo", &schema()).unwrap();
        assert_eq!(shown.secrets_set, ["token"]);
        assert!(!shown.values.contains_key("token"));
        assert_eq!(shown.values.get("region"), Some(&json!("us")));
        assert!(missing(&paths, "echo", &schema()).unwrap().is_empty());

        let config = std::fs::read_to_string(paths.config()).unwrap();
        assert!(config.contains("[plugins.echo]") && !config.contains("s3cret"), "{config}");
        let mode = std::fs::metadata(paths.secrets()).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn null_clears_and_invalid_values_are_rejected() {
        let (_dir, paths) = temp_paths();
        save(&paths, "echo", &schema(), &updates(json!({"token": "x", "region": "us"}))).unwrap();
        save(&paths, "echo", &schema(), &updates(json!({"token": null, "region": null}))).unwrap();
        let values = resolved(&paths, "echo", &schema()).unwrap();
        assert_eq!((values.get("token"), values.get("region")), (None, Some(&json!("eu"))));

        for bad in [json!({"region": "mars"}), json!({"verbose": "yes"}), json!({"limit": "5"}), json!({"nope": 1})] {
            let err = save(&paths, "echo", &schema(), &updates(bad.clone())).unwrap_err();
            assert_eq!(err.kind, ErrorKind::InvalidParams, "{bad}");
        }
    }
}
