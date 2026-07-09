//! Engine-specific compensation targets for transactional `dvandva upgrade`.

use std::fs;
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

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
}
