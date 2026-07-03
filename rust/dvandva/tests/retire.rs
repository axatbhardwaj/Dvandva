//! Integration tests for `dvandva retire-agents`, porting
//! `scripts/test-retire-standalone-agents.sh` (lettered cases a-k) plus a few
//! contract points called out in the port spec that the shell suite doesn't
//! exercise directly (regular-file-with-allowlisted-name, backup-path-not-a-
//! symlink, the new compiled default version).
//!
//! Every fixture lives under a per-test tempdir "fake HOME"; the real
//! `$HOME`/`$CODEX_HOME` are never touched.
//!
//! Version re-key: the shell suite pins `DVANDVA_EXPECTED_VERSION=1.1.0`
//! throughout. The Rust port moved the compiled default
//! (`dvandva::retire::DEFAULT_EXPECTED_VERSION`) to `1.2.0`, and the flow
//! patches move it to `1.3.0`, and the hardening slice moves it to `1.4.0`, the html-deliverables skill to `1.4.1`, and the wait-through-human docs wave to `1.4.2`. The fixture-based tests below set an
//! explicit `DVANDVA_EXPECTED_VERSION` override (see `EXPECTED_VER`, `1.2.0`)
//! and build their cache directories at that same explicit version, so they
//! are decoupled from the compiled default; only
//! `default_expected_version_is_1_4_2_when_env_unset` exercises the bare
//! default.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::json;

/// Re-keyed from the shell suite's "1.1.0" to the new compiled default.
const EXPECTED_VER: &str = "1.2.0";

const STANDALONE_AGENTS: [&str; 5] = [
    "adversarial-analyst.md",
    "architect.md",
    "developer.md",
    "quality-reviewer.md",
    "sandbox-executor.md",
];

const DVANDVA_AGENTS: [&str; 15] = [
    "adversarial-analyst.md",
    "architect.md",
    "baton-auditor.md",
    "cross-reviewer.md",
    "debugger.md",
    "deep-reviewer.md",
    "deslopper.md",
    "doc-verifier.md",
    "implementer.md",
    "integration-checker.md",
    "pattern-mapper.md",
    "researcher.md",
    "sandbox-verifier.md",
    "security-auditor.md",
    "test-creator.md",
];

#[derive(PartialEq, Clone, Copy)]
enum CacheCompleteness {
    Full,
    Partial,
}

struct Fixture {
    _root: tempfile::TempDir,
    root: PathBuf,
    haoshoku_dir: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root_dir = tempfile::tempdir().expect("tempdir");
        let root = root_dir.path().to_path_buf();

        let haoshoku_dir = root.join("haoshoku-sources");
        fs::create_dir_all(&haoshoku_dir).unwrap();
        for agent in STANDALONE_AGENTS {
            fs::write(
                haoshoku_dir.join(agent),
                format!("# fake haoshoku source: {agent}\n"),
            )
            .unwrap();
        }

        Fixture {
            _root: root_dir,
            root,
            haoshoku_dir,
        }
    }

    /// Build a fake HOME: `.claude/agents` with 5 symlinks + a decoy,
    /// `.claude/skills`, an optional dvandva plugin cache keyed at
    /// `EXPECTED_VER`, and empty Codex dirs.
    fn build_fake_home(
        &self,
        name: &str,
        include_cache: bool,
        cache_completeness: CacheCompleteness,
    ) -> PathBuf {
        let fake_home = self.root.join(name);

        let agents_dir = fake_home.join(".claude/agents");
        fs::create_dir_all(&agents_dir).unwrap();
        for agent in STANDALONE_AGENTS {
            symlink(self.haoshoku_dir.join(agent), agents_dir.join(agent)).unwrap();
        }
        fs::write(
            agents_dir.join("decoy.md"),
            "# decoy agent; must not be touched\n",
        )
        .unwrap();

        let skills_dir = fake_home.join(".claude/skills");
        fs::create_dir_all(&skills_dir).unwrap();
        fs::write(skills_dir.join("some-skill.md"), "# fake skill\n").unwrap();

        if include_cache {
            let cache_agents = fake_home
                .join(".claude/plugins/cache/dvandva/dvandva")
                .join(EXPECTED_VER)
                .join("agents");
            fs::create_dir_all(&cache_agents).unwrap();
            for agent in DVANDVA_AGENTS {
                fs::write(
                    cache_agents.join(agent),
                    format!("# fake dvandva agent: {agent}\n"),
                )
                .unwrap();
            }
            if cache_completeness == CacheCompleteness::Partial {
                fs::remove_file(cache_agents.join("debugger.md")).unwrap();
                fs::remove_file(cache_agents.join("pattern-mapper.md")).unwrap();
            }
        }

        for subdir in ["agents", "prompts", "subagents"] {
            fs::create_dir_all(fake_home.join(".codex").join(subdir)).unwrap();
        }

        fake_home
    }
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false)
}

fn find_retired_backup_dir(agents_dir: &Path) -> Option<PathBuf> {
    fs::read_dir(agents_dir)
        .ok()?
        .filter_map(Result::ok)
        .find(|entry| {
            entry.file_name().to_string_lossy().starts_with(".retired-") && entry.path().is_dir()
        })
        .map(|entry| entry.path())
}

fn run_retire(fake_home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dvandva"))
        .arg("retire-agents")
        .args(args)
        .env("HOME", fake_home)
        .env("CODEX_HOME", fake_home.join(".codex"))
        .env("DVANDVA_EXPECTED_VERSION", EXPECTED_VER)
        .output()
        .expect("failed to run dvandva retire-agents")
}

fn combined(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

// ---------------------------------------------------------------------------
// (a) Dry-run immutability
// ---------------------------------------------------------------------------
#[test]
fn dry_run_leaves_filesystem_untouched() {
    let fixture = Fixture::new();
    let fake_home = fixture.build_fake_home("home-dryrun", true, CacheCompleteness::Full);
    let agents_dir = fake_home.join(".claude/agents");

    let output = run_retire(&fake_home, &[]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    for agent in STANDALONE_AGENTS {
        assert!(
            is_symlink(&agents_dir.join(agent)),
            "{agent} symlink missing after dry-run"
        );
    }
    assert!(
        agents_dir.join("decoy.md").is_file(),
        "decoy.md missing after dry-run"
    );
    for agent in STANDALONE_AGENTS {
        assert!(
            fixture.haoshoku_dir.join(agent).is_file(),
            "haoshoku source {agent} missing after dry-run"
        );
    }
    assert!(
        find_retired_backup_dir(&agents_dir).is_none(),
        "unexpected .retired-* backup dir after dry-run"
    );

    let text = combined(&output);
    assert!(
        text.to_lowercase().contains("would retire"),
        "output should mention WOULD RETIRE: {text}"
    );
}

// ---------------------------------------------------------------------------
// (b) Apply moves exactly 5, writes manifest, leaves decoy + sources intact
// (c) Restore returns symlinks to original locations
// (h) Double restore guard
// ---------------------------------------------------------------------------
#[test]
fn apply_moves_five_writes_manifest_then_restore_roundtrips_and_double_restore_fails() {
    let fixture = Fixture::new();
    let fake_home = fixture.build_fake_home("home-roundtrip", true, CacheCompleteness::Full);
    let agents_dir = fake_home.join(".claude/agents");

    let apply_output = run_retire(&fake_home, &["--apply"]);
    assert_eq!(
        apply_output.status.code(),
        Some(0),
        "apply stderr: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    for agent in STANDALONE_AGENTS {
        let path = agents_dir.join(agent);
        assert!(
            !path.exists() && !is_symlink(&path),
            "{agent} still at original location after apply"
        );
    }

    let backup_dir = find_retired_backup_dir(&agents_dir).expect("backup dir created");

    for agent in STANDALONE_AGENTS {
        let backup_path = backup_dir.join(agent);
        assert!(
            is_symlink(&backup_path),
            "{agent} not found as symlink in backup dir"
        );
        let target = fs::read_link(&backup_path).unwrap();
        assert_eq!(target, fixture.haoshoku_dir.join(agent));
    }

    let manifest_path = backup_dir.join("manifest.json");
    assert!(
        manifest_path.is_file(),
        "manifest.json missing from backup dir"
    );
    let manifest_content = fs::read_to_string(&manifest_path).unwrap();
    let manifest_json: serde_json::Value = serde_json::from_str(&manifest_content).unwrap();
    assert!(manifest_json.is_object());

    for agent in STANDALONE_AGENTS {
        let orig_path = agents_dir.join(agent).display().to_string();
        assert!(
            manifest_content.contains(&orig_path),
            "manifest missing original_path for {agent}"
        );
        let backup_path = backup_dir.join(agent).display().to_string();
        assert!(
            manifest_content.contains(&backup_path),
            "manifest missing backup_path for {agent}"
        );
    }

    assert!(
        agents_dir.join("decoy.md").is_file(),
        "decoy.md missing after apply"
    );
    for agent in STANDALONE_AGENTS {
        assert!(fixture.haoshoku_dir.join(agent).is_file());
    }

    let apply_text = combined(&apply_output);
    assert!(
        apply_text.to_lowercase().contains("retired"),
        "apply output should mention RETIRED: {apply_text}"
    );

    // ----- (c) restore -----
    let restore_output = run_retire(&fake_home, &["--restore", backup_dir.to_str().unwrap()]);
    assert_eq!(
        restore_output.status.code(),
        Some(0),
        "restore stderr: {}",
        String::from_utf8_lossy(&restore_output.stderr)
    );

    for agent in STANDALONE_AGENTS {
        let path = agents_dir.join(agent);
        assert!(
            is_symlink(&path),
            "{agent} not restored to original location"
        );
        let target = fs::read_link(&path).unwrap();
        assert_eq!(target, fixture.haoshoku_dir.join(agent));
    }

    let restore_text = combined(&restore_output);
    assert!(
        restore_text.to_lowercase().contains("restored"),
        "restore output should mention RESTORED: {restore_text}"
    );
    assert!(
        restore_text.to_lowercase().contains("allowlist"),
        "restore output should mention allowlist validation: {restore_text}"
    );

    // ----- (h) double restore guard -----
    let second_restore = run_retire(&fake_home, &["--restore", backup_dir.to_str().unwrap()]);
    assert_ne!(
        second_restore.status.code(),
        Some(0),
        "second restore should exit nonzero"
    );
    let second_text = combined(&second_restore);
    assert!(
        second_text.to_lowercase().contains("already restored"),
        "second restore should mention already restored: {second_text}"
    );
}

// ---------------------------------------------------------------------------
// (d) Stale/missing cache -> parity gate refuses, exits nonzero, nothing moves
// ---------------------------------------------------------------------------
#[test]
fn apply_refuses_when_cache_missing() {
    let fixture = Fixture::new();
    let fake_home = fixture.build_fake_home("home-parity-no-cache", false, CacheCompleteness::Full);
    let agents_dir = fake_home.join(".claude/agents");

    let output = run_retire(&fake_home, &["--apply"]);
    assert_ne!(output.status.code(), Some(0));
    let text = combined(&output);
    assert!(text.to_lowercase().contains("parity"), "{text}");

    for agent in STANDALONE_AGENTS {
        assert!(
            is_symlink(&agents_dir.join(agent)),
            "{agent} was moved despite parity failure"
        );
    }
}

#[test]
fn apply_refuses_when_cache_incomplete() {
    let fixture = Fixture::new();
    let fake_home =
        fixture.build_fake_home("home-parity-partial", true, CacheCompleteness::Partial);
    let agents_dir = fake_home.join(".claude/agents");

    let output = run_retire(&fake_home, &["--apply"]);
    assert_ne!(output.status.code(), Some(0));
    let text = combined(&output);
    assert!(text.to_lowercase().contains("parity"), "{text}");

    for agent in STANDALONE_AGENTS {
        assert!(
            is_symlink(&agents_dir.join(agent)),
            "{agent} was moved despite partial-cache parity failure"
        );
    }
}

// ---------------------------------------------------------------------------
// (e) Skills dir untouched after apply
// ---------------------------------------------------------------------------
#[test]
fn skills_dir_untouched_after_apply() {
    let fixture = Fixture::new();
    let fake_home = fixture.build_fake_home("home-skills", true, CacheCompleteness::Full);

    let _ = run_retire(&fake_home, &["--apply"]);

    assert!(
        fake_home.join(".claude/skills/some-skill.md").is_file(),
        "skills dir was modified by apply"
    );
}

// ---------------------------------------------------------------------------
// (f) Codex empty dirs -> no-op report; never retires from Codex
// ---------------------------------------------------------------------------
#[test]
fn codex_dirs_report_no_op_in_dry_run_and_apply() {
    let fixture = Fixture::new();
    let fake_home = fixture.build_fake_home("home-codex", true, CacheCompleteness::Full);

    let dry_output = run_retire(&fake_home, &[]);
    assert_eq!(dry_output.status.code(), Some(0));
    let dry_text = combined(&dry_output);
    assert!(dry_text.to_lowercase().contains("no-op"), "{dry_text}");
    assert!(dry_text.to_lowercase().contains("codex"), "{dry_text}");

    let apply_output = run_retire(&fake_home, &["--apply"]);
    assert_eq!(
        apply_output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );
    let apply_text = combined(&apply_output);
    assert!(apply_text.to_lowercase().contains("no-op"), "{apply_text}");
}

// ---------------------------------------------------------------------------
// (g) Partial pre-existing retirement: absent allowlisted symlinks are
//     skipped; manifest tracks only what moved in this run.
// ---------------------------------------------------------------------------
#[test]
fn partial_pre_existing_retirement_skips_absent_and_tracks_only_moved() {
    let fixture = Fixture::new();
    let fake_home =
        fixture.build_fake_home("home-partial-preexisting", true, CacheCompleteness::Full);
    let agents_dir = fake_home.join(".claude/agents");

    fs::remove_file(agents_dir.join("adversarial-analyst.md")).unwrap();
    fs::remove_file(agents_dir.join("architect.md")).unwrap();

    let apply_output = run_retire(&fake_home, &["--apply"]);
    assert_eq!(
        apply_output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );
    let apply_text = combined(&apply_output);
    assert!(
        apply_text.contains("3 agent(s) retired"),
        "output should report 3 retired agents: {apply_text}"
    );

    let backup_dir = find_retired_backup_dir(&agents_dir).expect("backup dir created");
    let manifest_content = fs::read_to_string(backup_dir.join("manifest.json")).unwrap();
    let manifest_count = manifest_content.matches("\"original_path\"").count();
    assert_eq!(
        manifest_count, 3,
        "expected 3 manifest entries, got {manifest_count}"
    );

    for agent in ["developer.md", "quality-reviewer.md", "sandbox-executor.md"] {
        assert!(
            is_symlink(&backup_dir.join(agent)),
            "{agent} missing from backup"
        );
    }

    let restore_output = run_retire(&fake_home, &["--restore", backup_dir.to_str().unwrap()]);
    assert_eq!(
        restore_output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&restore_output.stderr)
    );

    for agent in ["developer.md", "quality-reviewer.md", "sandbox-executor.md"] {
        assert!(is_symlink(&agents_dir.join(agent)), "{agent} not restored");
    }
    for agent in ["adversarial-analyst.md", "architect.md"] {
        let path = agents_dir.join(agent);
        assert!(
            !path.exists() && !is_symlink(&path),
            "pre-existing absent {agent} was recreated"
        );
    }
}

// ---------------------------------------------------------------------------
// (i) Crafted/corrupted manifest must not restore outside the allowlist.
// ---------------------------------------------------------------------------
#[test]
fn restore_rejects_non_allowlisted_manifest_entry() {
    let fixture = Fixture::new();
    let fake_home = fixture.build_fake_home("home-crafted-manifest", true, CacheCompleteness::Full);
    let agents_dir = fake_home.join(".claude/agents");

    let backup_dir = agents_dir.join(".retired-crafted");
    fs::create_dir_all(&backup_dir).unwrap();
    let decoy_source = fixture.haoshoku_dir.join("decoy.md");
    fs::write(&decoy_source, "# fake haoshoku source: decoy.md\n").unwrap();
    symlink(&decoy_source, backup_dir.join("decoy.md")).unwrap();
    fs::remove_file(agents_dir.join("decoy.md")).unwrap();

    let manifest = json!({
        "retired_at": "test",
        "dvandva_version": EXPECTED_VER,
        "backup_dir": backup_dir.to_str().unwrap(),
        "entries": [{
            "original_path": agents_dir.join("decoy.md").to_str().unwrap(),
            "backup_path": backup_dir.join("decoy.md").to_str().unwrap(),
            "symlink_target": decoy_source.to_str().unwrap(),
        }]
    });
    fs::write(
        backup_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let output = run_retire(&fake_home, &["--restore", backup_dir.to_str().unwrap()]);
    assert_ne!(output.status.code(), Some(0));
    let text = combined(&output);
    assert!(
        text.to_lowercase().contains("invalid manifest entry"),
        "{text}"
    );

    let decoy_orig = agents_dir.join("decoy.md");
    assert!(
        !decoy_orig.exists() && !is_symlink(&decoy_orig),
        "decoy.md was restored from crafted manifest"
    );
    assert!(
        is_symlink(&backup_dir.join("decoy.md")),
        "crafted backup symlink was moved"
    );
}

#[test]
fn restore_rejects_mixed_manifest_before_moving_earlier_valid_entry() {
    let fixture = Fixture::new();
    let fake_home = fixture.build_fake_home("home-mixed-manifest", true, CacheCompleteness::Full);
    let agents_dir = fake_home.join(".claude/agents");

    let mixed_backup_dir = agents_dir.join(".retired-mixed");
    fs::create_dir_all(&mixed_backup_dir).unwrap();
    symlink(
        fixture.haoshoku_dir.join("developer.md"),
        mixed_backup_dir.join("developer.md"),
    )
    .unwrap();
    let decoy_source = fixture.haoshoku_dir.join("decoy.md");
    fs::write(&decoy_source, "# fake haoshoku source: decoy.md\n").unwrap();
    symlink(&decoy_source, mixed_backup_dir.join("decoy.md")).unwrap();
    fs::remove_file(agents_dir.join("developer.md")).unwrap();

    let manifest = json!({
        "retired_at": "test",
        "dvandva_version": EXPECTED_VER,
        "backup_dir": mixed_backup_dir.to_str().unwrap(),
        "entries": [
            {
                "original_path": agents_dir.join("developer.md").to_str().unwrap(),
                "backup_path": mixed_backup_dir.join("developer.md").to_str().unwrap(),
                "symlink_target": fixture.haoshoku_dir.join("developer.md").to_str().unwrap(),
            },
            {
                "original_path": agents_dir.join("decoy.md").to_str().unwrap(),
                "backup_path": mixed_backup_dir.join("decoy.md").to_str().unwrap(),
                "symlink_target": decoy_source.to_str().unwrap(),
            }
        ]
    });
    fs::write(
        mixed_backup_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let output = run_retire(
        &fake_home,
        &["--restore", mixed_backup_dir.to_str().unwrap()],
    );
    assert_ne!(output.status.code(), Some(0));

    assert!(
        is_symlink(&mixed_backup_dir.join("developer.md")),
        "valid backup moved before later invalid entry was rejected"
    );
    let developer_orig = agents_dir.join("developer.md");
    assert!(
        !developer_orig.exists() && !is_symlink(&developer_orig),
        "valid original was restored before mixed manifest rejection"
    );
    assert!(
        is_symlink(&mixed_backup_dir.join("decoy.md")),
        "mixed decoy backup was moved"
    );
}

// ---------------------------------------------------------------------------
// (j)/(k) Manifest JSON must be parser-valid and paths with quotes/spaces
// must round-trip.
// ---------------------------------------------------------------------------
fn roundtrip_with_home_name(name: &str) {
    let fixture = Fixture::new();
    let fake_home = fixture.build_fake_home(name, true, CacheCompleteness::Full);
    let agents_dir = fake_home.join(".claude/agents");

    let apply_output = run_retire(&fake_home, &["--apply"]);
    assert_eq!(
        apply_output.status.code(),
        Some(0),
        "apply stderr: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let backup_dir = find_retired_backup_dir(&agents_dir).expect("backup dir created");
    let manifest_content = fs::read_to_string(backup_dir.join("manifest.json")).unwrap();
    assert!(
        serde_json::from_str::<serde_json::Value>(&manifest_content).is_ok(),
        "manifest is not valid JSON"
    );

    let restore_output = run_retire(&fake_home, &["--restore", backup_dir.to_str().unwrap()]);
    assert_eq!(
        restore_output.status.code(),
        Some(0),
        "restore stderr: {}",
        String::from_utf8_lossy(&restore_output.stderr)
    );

    for agent in STANDALONE_AGENTS {
        assert!(
            is_symlink(&agents_dir.join(agent)),
            "{agent} not restored under {name}"
        );
    }
}

#[test]
fn manifest_json_roundtrips_home_path_with_quote() {
    roundtrip_with_home_name("home-with-\"quote");
}

#[test]
fn manifest_json_roundtrips_home_path_with_space() {
    roundtrip_with_home_name("home with space");
}

// ---------------------------------------------------------------------------
// Contract point: only symlinks are touched; a regular file with an
// allowlisted name must be refused, not retired.
// ---------------------------------------------------------------------------
#[test]
fn regular_file_with_allowlisted_name_is_skipped_not_retired() {
    let fixture = Fixture::new();
    let fake_home = fixture.build_fake_home("home-regular-file", true, CacheCompleteness::Full);
    let agents_dir = fake_home.join(".claude/agents");

    fs::remove_file(agents_dir.join("developer.md")).unwrap();
    fs::write(agents_dir.join("developer.md"), "# not a symlink\n").unwrap();

    let dry_output = run_retire(&fake_home, &[]);
    let dry_text = combined(&dry_output);
    assert!(
        dry_text.contains("SKIP (not a symlink): developer.md"),
        "{dry_text}"
    );

    let apply_output = run_retire(&fake_home, &["--apply"]);
    assert_eq!(
        apply_output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );
    let apply_text = combined(&apply_output);
    assert!(
        apply_text.contains("SKIP (not a symlink): developer.md"),
        "{apply_text}"
    );

    let developer_path = agents_dir.join("developer.md");
    assert!(
        developer_path.is_file() && !is_symlink(&developer_path),
        "regular file should remain untouched, not retired"
    );

    let backup_dir =
        find_retired_backup_dir(&agents_dir).expect("backup dir created for the other 4");
    assert!(
        !backup_dir.join("developer.md").exists(),
        "regular file should not appear in backup dir"
    );
}

// ---------------------------------------------------------------------------
// Contract point: restore validates that backup entries are symlinks.
// ---------------------------------------------------------------------------
#[test]
fn restore_rejects_backup_path_that_is_not_a_symlink() {
    let fixture = Fixture::new();
    let fake_home =
        fixture.build_fake_home("home-backup-not-symlink", true, CacheCompleteness::Full);
    let agents_dir = fake_home.join(".claude/agents");

    let backup_dir = agents_dir.join(".retired-bad-backup");
    fs::create_dir_all(&backup_dir).unwrap();
    fs::write(backup_dir.join("developer.md"), "# not a symlink\n").unwrap();
    fs::remove_file(agents_dir.join("developer.md")).unwrap();

    let manifest = json!({
        "retired_at": "test",
        "dvandva_version": EXPECTED_VER,
        "backup_dir": backup_dir.to_str().unwrap(),
        "entries": [{
            "original_path": agents_dir.join("developer.md").to_str().unwrap(),
            "backup_path": backup_dir.join("developer.md").to_str().unwrap(),
            "symlink_target": fixture.haoshoku_dir.join("developer.md").to_str().unwrap(),
        }]
    });
    fs::write(
        backup_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let output = run_retire(&fake_home, &["--restore", backup_dir.to_str().unwrap()]);
    assert_ne!(output.status.code(), Some(0));
    let text = combined(&output);
    assert!(
        text.to_lowercase().contains("invalid manifest entry"),
        "{text}"
    );
    assert!(text.contains("not a symlink"), "{text}");

    let orig = agents_dir.join("developer.md");
    assert!(
        !orig.exists() && !is_symlink(&orig),
        "original should remain absent after rejection"
    );
}

// ---------------------------------------------------------------------------
// Contract point: DVANDVA_EXPECTED_VERSION default changes to 1.4.2 in the
// wait-through-human docs wave (S2/S4/S5/S6 hardening pinned 1.4.0; the
// html-deliverables skill 1.4.1; the flow patches 1.3.0; the Rust port 1.2.0;
// the shell suite 1.1.0).
// ---------------------------------------------------------------------------
#[test]
fn default_expected_version_is_1_4_2_when_env_unset() {
    let fixture = Fixture::new();
    let fake_home = fixture.build_fake_home("home-default-version", false, CacheCompleteness::Full);
    let cache_agents = fake_home.join(".claude/plugins/cache/dvandva/dvandva/1.4.2/agents");
    fs::create_dir_all(&cache_agents).unwrap();
    for agent in DVANDVA_AGENTS {
        fs::write(
            cache_agents.join(agent),
            format!("# fake dvandva agent: {agent}\n"),
        )
        .unwrap();
    }

    let output = Command::new(env!("CARGO_BIN_EXE_dvandva"))
        .arg("retire-agents")
        .arg("--apply")
        .env("HOME", &fake_home)
        .env("CODEX_HOME", fake_home.join(".codex"))
        .env_remove("DVANDVA_EXPECTED_VERSION")
        .output()
        .expect("failed to run dvandva retire-agents");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = combined(&output);
    assert!(text.contains("1.4.2"), "{text}");
}

// ---------------------------------------------------------------------------
// Argument parsing / usage exit codes.
// ---------------------------------------------------------------------------
#[test]
fn unknown_flag_exits_2_with_usage() {
    let fixture = Fixture::new();
    let fake_home = fixture.build_fake_home("home-bad-flag", true, CacheCompleteness::Full);

    let output = run_retire(&fake_home, &["--bogus"]);
    assert_eq!(output.status.code(), Some(2));
    let text = combined(&output);
    assert!(text.contains("unknown option"), "{text}");
    assert!(text.contains("Usage:"), "{text}");
}

#[test]
fn restore_missing_argument_exits_1() {
    let fixture = Fixture::new();
    let fake_home = fixture.build_fake_home("home-restore-no-arg", true, CacheCompleteness::Full);

    let output = run_retire(&fake_home, &["--restore"]);
    assert_eq!(output.status.code(), Some(1));
    let text = combined(&output);
    assert!(
        text.contains("--restore requires a backup directory argument"),
        "{text}"
    );
}

// ---------------------------------------------------------------------------
// (B9) An explicit empty-string --restore value must be rejected at
// dispatch time with the shell's exact message, instead of proceeding and
// failing later with a confusing "Manifest not found: /manifest.json".
// ---------------------------------------------------------------------------
#[test]
fn restore_empty_string_argument_exits_1() {
    let fixture = Fixture::new();
    let fake_home =
        fixture.build_fake_home("home-restore-empty-arg", true, CacheCompleteness::Full);

    let output = run_retire(&fake_home, &["--restore", ""]);
    assert_eq!(output.status.code(), Some(1));
    let text = combined(&output);
    assert!(
        text.contains("ERROR: --restore requires a backup directory"),
        "{text}"
    );
    assert!(
        !text.contains("backup directory argument"),
        "empty-string case must not reuse the missing-argument message: {text}"
    );
    assert!(
        !text.contains("Manifest not found"),
        "must reject before attempting to read a manifest: {text}"
    );
}

#[test]
fn help_flag_exits_0() {
    let fixture = Fixture::new();
    let fake_home = fixture.build_fake_home("home-help", true, CacheCompleteness::Full);

    let output = run_retire(&fake_home, &["-h"]);
    assert_eq!(output.status.code(), Some(0));
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("Usage:"), "{text}");
}
