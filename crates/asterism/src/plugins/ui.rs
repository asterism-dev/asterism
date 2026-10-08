use std::path::{Component, Path};

use asterism_proto::types::PluginOrigin;

use super::registry::Plugin;

/// UI files of the built-in plugins, compiled in like their manifests: (plugin, path, contents).
pub const BUILTIN_UI: &[(&str, &str, &str)] = &[
    (
        "agents",
        "ui/agents.html",
        include_str!("../../plugins/agents/ui/agents.html"),
    ),
    (
        "agents",
        "ui/agents.css",
        include_str!("../../plugins/agents/ui/agents.css"),
    ),
    (
        "agents",
        "ui/agents.mjs",
        include_str!("../../plugins/agents/ui/agents.mjs"),
    ),
    (
        "agents",
        "ui/tree.mjs",
        include_str!("../../plugins/agents/ui/tree.mjs"),
    ),
];

pub fn is_safe_path(path: &str) -> bool {
    !path.is_empty()
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

pub fn read(plugin: &Plugin, path: &str) -> Option<Vec<u8>> {
    if !is_safe_path(path) {
        return None;
    }
    if plugin.origin == PluginOrigin::Builtin {
        return BUILTIN_UI
            .iter()
            .find(|(name, file, _)| *name == plugin.name && *file == path)
            .map(|(_, _, contents)| contents.as_bytes().to_vec());
    }
    let dir = plugin.dir.canonicalize().ok()?;
    let file = dir.join(path).canonicalize().ok()?;
    if !file.starts_with(&dir) || !file.is_file() {
        return None;
    }
    std::fs::read(file).ok()
}

pub fn mime(path: &str) -> &'static str {
    match Path::new(path).extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::registry::Status;

    fn plugin(dir: &Path, origin: PluginOrigin) -> Plugin {
        Plugin {
            name: "p".into(),
            origin,
            dir: dir.to_path_buf(),
            manifest: None,
            status: Status::Ok,
        }
    }

    #[test]
    fn reads_files_inside_the_plugin_dir_only() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("p");
        std::fs::create_dir_all(dir.join("ui")).unwrap();
        std::fs::write(dir.join("ui/a.html"), "<p>").unwrap();
        std::fs::write(root.path().join("secret"), "s").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.path().join("secret"), dir.join("ui/link")).unwrap();
        let p = plugin(&dir, PluginOrigin::Linked);
        assert_eq!(read(&p, "ui/a.html").as_deref(), Some(&b"<p>"[..]));
        for bad in [
            "../secret",
            "/etc/hosts",
            "ui/../../secret",
            "ui/link",
            "ui/missing",
            "",
        ] {
            assert_eq!(read(&p, bad), None, "{bad}");
        }
    }

    #[test]
    fn guesses_mime_types() {
        assert_eq!(mime("a/b.html"), "text/html; charset=utf-8");
        assert_eq!(mime("b.mjs"), "text/javascript; charset=utf-8");
        assert_eq!(mime("c.css"), "text/css; charset=utf-8");
        assert_eq!(mime("d.bin"), "application/octet-stream");
    }
}
