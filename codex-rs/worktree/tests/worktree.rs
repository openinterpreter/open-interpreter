#![allow(clippy::expect_used)]

use codex_worktree::WorktreeManager;
use codex_worktree::WorktreeSettings;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

struct RepositoryFixture {
    _temp_dir: TempDir,
    codex_home: PathBuf,
    repository: PathBuf,
}

impl RepositoryFixture {
    fn new() -> Self {
        let temp_dir = tempfile::tempdir().expect("create temporary test directory");
        let temp_root = dunce::canonicalize(temp_dir.path()).expect("canonicalize temporary root");
        let codex_home = temp_root.join("codex-home");
        let repository = temp_root.join("project");
        fs::create_dir_all(&codex_home).expect("create Codex home");
        initialize_repository(&repository);

        Self {
            _temp_dir: temp_dir,
            codex_home,
            repository,
        }
    }

    fn manager(&self) -> WorktreeManager {
        let settings =
            WorktreeSettings::from_desktop_config(&self.codex_home, /*desktop*/ None)
                .expect("load default worktree settings");
        WorktreeManager::new(settings)
    }
}

fn initialize_repository(repository: &Path) {
    fs::create_dir_all(repository.join("nested/component"))
        .expect("create temporary repository directories");
    run_git(repository, &["init", "--quiet"]);
    fs::write(
        repository.join("README.md"),
        "initial repository contents\n",
    )
    .expect("write tracked repository file");
    fs::write(
        repository.join("nested/component/tracked.txt"),
        "nested tracked file\n",
    )
    .expect("write nested tracked repository file");
    run_git(repository, &["add", "."]);
    commit(repository, "initial commit");
}

fn commit(repository: &Path, message: &str) {
    run_git(
        repository,
        &["commit", "--quiet", "--no-gpg-sign", "-m", message],
    );
}

fn git_output(repository: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .current_dir(repository)
        .args([
            "-c",
            "user.name=Codex Worktree Test",
            "-c",
            "user.email=codex-worktree-test@example.invalid",
            "-c",
            "commit.gpgSign=false",
            "-c",
            "tag.gpgSign=false",
        ])
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("run git {args:?} in {repository:?}: {error}"))
}

fn run_git(repository: &Path, args: &[&str]) -> String {
    let output = git_output(repository, args);
    assert!(
        output.status.success(),
        "git {args:?} failed in {repository:?}: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    String::from_utf8(output.stdout)
        .expect("git output is UTF-8")
        .trim()
        .to_owned()
}

#[test]
fn thread_binding_rejects_primary_and_unmanaged_worktrees() {
    let fixture = RepositoryFixture::new();
    let manager = fixture.manager();
    fs::create_dir_all(&manager.settings().root).expect("create managed root");
    assert!(
        manager
            .bind_thread(&fixture.repository, "not-a-managed-thread")
            .is_err(),
    );
    assert!(
        !fixture.repository.join(".git/codex-thread.json").exists(),
        "primary checkout ownership metadata must not be modified",
    );

    let unmanaged = manager.settings().root.join("scratch").join("project");
    fs::create_dir_all(unmanaged.parent().expect("unmanaged parent"))
        .expect("create unmanaged parent");
    run_git(
        &fixture.repository,
        &[
            "worktree",
            "add",
            "--detach",
            unmanaged.to_str().expect("UTF-8 unmanaged worktree"),
            "HEAD",
        ],
    );
    assert!(manager.bind_thread(&unmanaged, "unmanaged").is_err());
    assert!(manager.owner(&unmanaged).is_err());
}

#[test]
fn thread_binding_writes_the_exact_desktop_owner_schema_into_git_metadata() {
    let fixture = RepositoryFixture::new();
    let manager = fixture.manager();
    let checkout = manager.settings().root.join("a1b2/project");
    fs::create_dir_all(checkout.parent().expect("checkout parent")).expect("create bucket");
    run_git(
        &fixture.repository,
        &[
            "worktree",
            "add",
            "--detach",
            checkout.to_str().expect("UTF-8 checkout"),
            "HEAD",
        ],
    );
    let thread_id = "019f1234-5678-7000-8000-000000000001";

    assert_eq!(manager.owner(&checkout).expect("read initial owner"), None);

    manager
        .bind_thread(&checkout, thread_id)
        .expect("bind managed worktree to thread");

    assert_eq!(
        manager.owner(&checkout).expect("read bound owner"),
        Some(thread_id.to_owned()),
    );

    let metadata_path = checkout.join(run_git(
        &checkout,
        &["rev-parse", "--git-path", "codex-thread.json"],
    ));
    let metadata: Value = serde_json::from_slice(
        &fs::read(&metadata_path).expect("read Desktop-compatible worktree owner metadata"),
    )
    .expect("parse Desktop-compatible worktree owner metadata");

    assert_eq!(
        metadata,
        json!({
            "version": 1,
            "ownerThreadId": thread_id,
        }),
    );
}
