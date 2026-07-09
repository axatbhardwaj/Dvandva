//! Engine-specific compensation targets for transactional `dvandva upgrade`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// Filesystem state that engine plugin upgrade compensation must snapshot at W0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineStateTargets {
    pub claude_installed_plugins: PathBuf,
    pub claude_cache_base: PathBuf,
    pub codex_marketplace_tmp: PathBuf,
    pub codex_config: PathBuf,
    pub codex_cache_base: PathBuf,
}

/// Compute every engine-owned path whose W0 state matters for rollback.
pub fn engine_state_targets(
    home: &Path,
    codex_home: &Path,
    marketplace: impl AsRef<Path>,
) -> EngineStateTargets {
    EngineStateTargets {
        claude_installed_plugins: home.join(".claude/plugins/installed_plugins.json"),
        claude_cache_base: home.join(".claude/plugins/cache/dvandva/dvandva"),
        codex_marketplace_tmp: codex_marketplace_cache_dir(codex_home, marketplace),
        codex_config: codex_home.join("config.toml"),
        codex_cache_base: codex_home.join("plugins/cache/dvandva/dvandva"),
    }
}

/// Compute Codex's marketplace tmp/cache checkout directory without deleting it.
pub fn codex_marketplace_cache_dir(codex_home: &Path, marketplace: impl AsRef<Path>) -> PathBuf {
    codex_home
        .join(".tmp/marketplaces")
        .join(marketplace_cache_name(marketplace.as_ref()))
}

/// Restore only Claude Code's active Dvandva plugin pointer from a W0 snapshot.
///
/// The snapshot should be the pre-upgrade `installed_plugins.json`; unrelated
/// plugins in the current file are preserved.
pub fn restore_claude_dvandva_pointer(current_path: &Path, snapshot_path: &Path) -> io::Result<()> {
    let mut current = read_json(current_path)?;
    let snapshot = read_json(snapshot_path)?;
    let replacement = find_dvandva_entry(&snapshot).cloned().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} has no dvandva plugin entry", snapshot_path.display()),
        )
    })?;

    if !replace_dvandva_entry(&mut current, replacement.clone()) {
        insert_dvandva_entry(&mut current, replacement)?;
    }

    let body = serde_json::to_vec_pretty(&current).map_err(io::Error::other)?;
    fs::write(current_path, body)
}

fn marketplace_cache_name(marketplace: &Path) -> String {
    local_marketplace_name(marketplace).unwrap_or_else(|| {
        let raw = marketplace.to_string_lossy();
        let trimmed = raw.strip_suffix(".git").unwrap_or(&raw);
        Path::new(trimmed)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(trimmed)
            .to_ascii_lowercase()
    })
}

fn local_marketplace_name(marketplace: &Path) -> Option<String> {
    if !marketplace.is_dir() {
        return None;
    }
    let manifest = fs::read_to_string(marketplace.join(".agents/plugins/marketplace.json")).ok()?;
    let value: Value = serde_json::from_str(&manifest).ok()?;
    value
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn read_json(path: &Path) -> io::Result<Value> {
    let body = fs::read(path)?;
    serde_json::from_slice(&body).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

fn find_dvandva_entry(value: &Value) -> Option<&Value> {
    if let Some(entry) = value.get("dvandva") {
        return Some(entry);
    }
    let plugins = value.get("plugins")?;
    if let Some(entry) = plugins.get("dvandva") {
        return Some(entry);
    }
    plugins
        .as_array()?
        .iter()
        .find(|entry| plugin_is_dvandva(entry))
}

fn replace_dvandva_entry(value: &mut Value, replacement: Value) -> bool {
    if let Some(object) = value.as_object_mut() {
        if object.contains_key("dvandva") {
            object.insert("dvandva".to_string(), replacement);
            return true;
        }
        if let Some(plugins) = object.get_mut("plugins") {
            if let Some(plugin_object) = plugins.as_object_mut() {
                if plugin_object.contains_key("dvandva") {
                    plugin_object.insert("dvandva".to_string(), replacement);
                    return true;
                }
            }
            if let Some(plugin_array) = plugins.as_array_mut() {
                for entry in plugin_array {
                    if plugin_is_dvandva(entry) {
                        *entry = replacement;
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn insert_dvandva_entry(value: &mut Value, replacement: Value) -> io::Result<()> {
    let object = value.as_object_mut().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "installed_plugins.json root must be an object",
        )
    })?;
    match object.get_mut("plugins").and_then(Value::as_object_mut) {
        Some(plugins) => {
            plugins.insert("dvandva".to_string(), replacement);
        }
        None => {
            object.insert("dvandva".to_string(), replacement);
        }
    }
    Ok(())
}

fn plugin_is_dvandva(value: &Value) -> bool {
    ["name", "id", "plugin", "pluginName"]
        .iter()
        .any(|key| value.get(*key).and_then(Value::as_str) == Some("dvandva"))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use serde_json::json;

    use super::*;

    #[test]
    fn engine_targets_cover_claude_and_codex_state() {
        let home = Path::new("/home/test");
        let codex_home = Path::new("/tmp/codex-home");

        let targets = engine_state_targets(home, codex_home, "axatbhardwaj/Dvandva");

        assert_eq!(
            targets.claude_installed_plugins,
            PathBuf::from("/home/test/.claude/plugins/installed_plugins.json")
        );
        assert_eq!(
            targets.claude_cache_base,
            PathBuf::from("/home/test/.claude/plugins/cache/dvandva/dvandva")
        );
        assert_eq!(
            targets.codex_marketplace_tmp,
            PathBuf::from("/tmp/codex-home/.tmp/marketplaces/dvandva")
        );
        assert_eq!(
            targets.codex_config,
            PathBuf::from("/tmp/codex-home/config.toml")
        );
        assert_eq!(
            targets.codex_cache_base,
            PathBuf::from("/tmp/codex-home/plugins/cache/dvandva/dvandva")
        );
    }

    #[test]
    fn local_marketplace_cache_dir_uses_manifest_name() {
        let tmp = tempfile::tempdir().unwrap();
        let marketplace = tmp.path().join("not-the-cache-name");
        fs::create_dir_all(marketplace.join(".agents/plugins")).unwrap();
        fs::write(
            marketplace.join(".agents/plugins/marketplace.json"),
            r#"{"name":"dvandva","plugins":[]}"#,
        )
        .unwrap();

        assert_eq!(
            codex_marketplace_cache_dir(Path::new("/tmp/codex-home"), &marketplace),
            PathBuf::from("/tmp/codex-home/.tmp/marketplaces/dvandva")
        );
    }

    #[test]
    fn restore_claude_pointer_rewrites_only_dvandva_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let current = tmp.path().join("current.json");
        let snapshot = tmp.path().join("snapshot.json");
        fs::write(
            &current,
            serde_json::to_vec_pretty(&json!({
                "plugins": {
                    "dvandva": {
                        "version": "1.5.2",
                        "installPath": "/new",
                        "gitCommitSha": "new-sha"
                    },
                    "other": {"version": "9"}
                }
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            &snapshot,
            serde_json::to_vec_pretty(&json!({
                "plugins": {
                    "dvandva": {
                        "version": "1.5.1",
                        "installPath": "/old",
                        "gitCommitSha": "old-sha"
                    },
                    "other": {"version": "1"}
                }
            }))
            .unwrap(),
        )
        .unwrap();

        restore_claude_dvandva_pointer(&current, &snapshot).unwrap();

        let restored: serde_json::Value =
            serde_json::from_slice(&fs::read(&current).unwrap()).unwrap();
        assert_eq!(restored["plugins"]["dvandva"]["version"], "1.5.1");
        assert_eq!(restored["plugins"]["dvandva"]["installPath"], "/old");
        assert_eq!(restored["plugins"]["dvandva"]["gitCommitSha"], "old-sha");
        assert_eq!(restored["plugins"]["other"]["version"], "9");
    }
}
