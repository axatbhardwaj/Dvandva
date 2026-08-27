use assert_cmd::Command;

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dvandva-v4"))
}

fn git(repo: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_repo(origin: &str) -> tempfile::TempDir {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "--quiet"]);
    git(repo.path(), &["remote", "add", "origin", origin]);
    repo
}

fn identify(repo: &std::path::Path) -> serde_json::Value {
    let output = command()
        .args(["identify", "--workspace", repo.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn github_https_and_scp_origins_share_one_repository_identity() {
    let https = init_repo("https://github.com/AxatBhardwaj/Dvandva.git");
    let scp = init_repo("git@github.com:AxatBhardwaj/Dvandva.git");

    let https_identity = identify(https.path());
    let scp_identity = identify(scp.path());

    assert_eq!(
        https_identity["repository_id"],
        "github.com/axatbhardwaj/dvandva"
    );
    assert_eq!(
        scp_identity["repository_id"],
        "github.com/axatbhardwaj/dvandva"
    );
    assert_eq!(
        https_identity["origin"],
        "https://github.com/AxatBhardwaj/Dvandva.git"
    );
    assert_eq!(
        scp_identity["origin"],
        "git@github.com:AxatBhardwaj/Dvandva.git"
    );
}

#[test]
fn linked_worktrees_share_a_local_repository_fingerprint() {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "--quiet"]);
    git(repo.path(), &["config", "user.name", "Dvandva Test"]);
    git(
        repo.path(),
        &["config", "user.email", "dvandva@example.test"],
    );
    std::fs::write(repo.path().join("README.md"), "test\n").unwrap();
    git(repo.path(), &["add", "README.md"]);
    git(repo.path(), &["commit", "--quiet", "-m", "initial"]);

    let linked_root = tempfile::tempdir().unwrap();
    let linked = linked_root.path().join("review-worktree");
    git(
        repo.path(),
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            "review-worktree",
            linked.to_str().unwrap(),
        ],
    );

    let primary = identify(repo.path());
    let reviewer = identify(&linked);
    assert_eq!(primary["repository_id"], reviewer["repository_id"]);
    assert!(primary["repository_id"]
        .as_str()
        .unwrap()
        .starts_with("local:"));
    assert_ne!(primary["worktree"], reviewer["worktree"]);
    assert_eq!(primary["origin"], serde_json::Value::Null);
    assert_eq!(reviewer["origin"], serde_json::Value::Null);
}

#[test]
fn non_git_directories_fail_with_a_repository_diagnostic() {
    let workspace = tempfile::tempdir().unwrap();
    command()
        .args([
            "identify",
            "--workspace",
            workspace.path().to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains(r#""error":"repository_missing""#));
}

#[test]
fn unsupported_origins_fail_closed() {
    let repo = init_repo("not-a-supported-remote");
    command()
        .args(["identify", "--workspace", repo.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicates::str::contains(r#""error":"invalid_origin""#));
}
