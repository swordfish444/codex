#![cfg(target_os = "macos")]
#![expect(clippy::unwrap_used)]

use std::collections::HashMap;
use std::path::Path;

use codex_core::protocol::SandboxPolicy;
use codex_core::seatbelt::spawn_command_under_seatbelt;
use codex_core::spawn::StdioPolicy;
use tempfile::TempDir;

#[tokio::test]
async fn workspace_write_restricts_git_and_parent() {
    let outer = TempDir::new().unwrap();
    let repo = outer.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    std::fs::create_dir(repo.join(".git")).unwrap();

    assert!(
        preflight_can_write(&repo).await,
        "seatbelt preflight failed: sandbox unusable or policy not permitting write"
    );

    let policy = SandboxPolicy::new_workspace_write_policy();

    assert!(touch("file", &repo, &policy).await);
    assert!(repo.join("file").exists());

    let blocked_git = repo.join(".git/blocked");
    assert!(!touch_abs(&blocked_git, &repo, &policy).await);
    assert!(!repo.join(".git/blocked").exists());
    let parent_path = outer.path().join("parent");
    assert!(!touch_abs(&parent_path, &repo, &policy).await);
    assert!(!outer.path().join("parent").exists());
}

#[tokio::test]
async fn danger_full_access_allows_git_and_parent() {
    let outer = TempDir::new().unwrap();
    let repo = outer.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    std::fs::create_dir(repo.join(".git")).unwrap();

    assert!(
        preflight_can_write(&repo).await,
        "seatbelt preflight failed: sandbox unusable or policy not permitting write"
    );

    let policy = SandboxPolicy::DangerFullAccess;

    assert!(touch(".git/allowed", &repo, &policy).await);
    assert!(repo.join(".git/allowed").exists());

    assert!(touch("../parent_ok", &repo, &policy).await);
    assert!(outer.path().join("parent_ok").exists());
}

#[tokio::test]
async fn read_only_forbids_writing() {
    let outer = TempDir::new().unwrap();
    let repo = outer.path().join("repo");
    std::fs::create_dir(&repo).unwrap();

    assert!(
        preflight_can_write(&repo).await,
        "seatbelt preflight failed: sandbox unusable or policy not permitting write"
    );

    let policy = SandboxPolicy::ReadOnly;

    assert!(!touch("file", &repo, &policy).await);
    assert!(!repo.join("file").exists());
}

#[tokio::test]
async fn workspace_write_allows_repo_but_blocks_sub_git_and_parent() {
    let outer = TempDir::new().unwrap();
    let repo = outer.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    // Create .git at the repo root so writes under it are denied.
    std::fs::create_dir(repo.join(".git")).unwrap();
    // Also create a subdirectory that looks like a nested repo to verify that
    // writes to any ".git" subtree under the repo are denied.
    std::fs::create_dir_all(repo.join("subdir/.git")).unwrap();

    assert!(
        preflight_can_write(&repo).await,
        "seatbelt preflight failed: sandbox unusable or policy not permitting write"
    );

    let policy = SandboxPolicy::new_workspace_write_policy();

    // Writing at repo root is allowed.
    assert!(touch("ok_at_root", &repo, &policy).await);
    assert!(repo.join("ok_at_root").exists());

    // Writing in a regular subdir is allowed.
    assert!(touch("subdir/ok", &repo, &policy).await);
    assert!(repo.join("subdir/ok").exists());

    // But writing anywhere under .git (root) is blocked.
    let blocked_root = repo.join(".git/blocked_root");
    assert!(!touch_abs(&blocked_root, &repo, &policy).await);
    assert!(!repo.join(".git/blocked_root").exists());

    // And writing under a nested .git directory is blocked.
    let blocked_nested = repo.join("subdir/.git/blocked_nested");
    assert!(!touch_abs(&blocked_nested, &repo, &policy).await);
    assert!(!repo.join("subdir/.git/blocked_nested").exists());

    // Parent of the repo must not be writable via WorkspaceWrite.
    let parent_blocked = outer.path().join("parent_should_be_blocked");
    assert!(!touch_abs(&parent_blocked, &repo, &policy).await);
    assert!(!outer.path().join("parent_should_be_blocked").exists());
}

#[tokio::test]
async fn workspace_write_allows_tmpdir() {
    let repo_dir = TempDir::new().unwrap();
    let repo_path = repo_dir.path();
    // Ensure repo root exists but no actual .git required for this test.
    let policy = SandboxPolicy::new_workspace_write_policy();

    assert!(
        preflight_can_write(repo_path).await,
        "seatbelt preflight failed: sandbox unusable or policy not permitting write"
    );

    let tmp = std::env::temp_dir();
    // Attempt to write into the system temp dir which should be writable on macOS
    // because TMPDIR is whitelisted by the WorkspaceWrite policy.
    let dest = tmp.join("codex_workspace_write_tmp_test_file");
    let rel = dest.to_string_lossy().to_string();
    let mut child = spawn_command_under_seatbelt(
        vec!["/usr/bin/touch".to_string(), rel],
        &policy,
        repo_path.to_path_buf(),
        StdioPolicy::RedirectForShellTool,
        HashMap::new(),
    )
    .await
    .unwrap();
    assert!(child.wait().await.unwrap().success());
    assert!(dest.exists());
}

#[tokio::test]
async fn workspace_write_extra_root_git_protected() {
    let parent = TempDir::new().unwrap();
    let repo = parent.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    std::fs::create_dir(repo.join(".git")).unwrap();

    assert!(
        preflight_can_write(&repo).await,
        "seatbelt preflight failed: sandbox unusable or policy not permitting write"
    );

    let other = TempDir::new().unwrap();
    let other_root = other.path().to_path_buf();
    std::fs::create_dir_all(other_root.join(".git")).unwrap();

    let policy = SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![other_root.clone()],
        network_access: false,
        use_exact_writable_roots: true,
    };

    // Run from repo so cwd is writable and .git protected there as well.
    assert!(touch("ok", &repo, &policy).await);
    assert!(repo.join("ok").exists());
    let blocked_git = repo.join(".git/blocked");
    assert!(!touch_abs(&blocked_git, &repo, &policy).await);
    assert!(!repo.join(".git/blocked").exists());

    // The additional writable root should also be writable, but its .git should be blocked.
    let ok_in_other = other.path().join("allowed");
    let blocked_in_other = other.path().join(".git/blocked");

    let mut child = spawn_command_under_seatbelt(
        vec![
            "/usr/bin/touch".to_string(),
            ok_in_other.to_string_lossy().to_string(),
        ],
        &policy,
        repo.clone(),
        StdioPolicy::RedirectForShellTool,
        HashMap::new(),
    )
    .await
    .unwrap();
    assert!(child.wait().await.unwrap().success());
    assert!(ok_in_other.exists());

    let mut child = spawn_command_under_seatbelt(
        vec![
            "/usr/bin/touch".to_string(),
            blocked_in_other.to_string_lossy().to_string(),
        ],
        &policy,
        repo,
        StdioPolicy::RedirectForShellTool,
        HashMap::new(),
    )
    .await
    .unwrap();
    assert!(!child.wait().await.unwrap().success());
    assert!(!blocked_in_other.exists());
}

fn sandbox_exec_present() -> bool {
    std::path::Path::new("/usr/bin/sandbox-exec").exists()
}

fn sandbox_exec_usable() -> bool {
    if !sandbox_exec_present() {
        return false;
    }
    match std::process::Command::new("/usr/bin/sandbox-exec")
        .args(["-p", "(version 1)\n(allow default)", "--", "/usr/bin/true"])
        .status()
    {
        Ok(status) => status.success(),
        Err(_) => false,
    }
}

async fn preflight_can_write(cwd: &Path) -> bool {
    if !sandbox_exec_usable() {
        return false;
    }
    // Verify a simple write under a standard WorkspaceWrite policy. If even this
    // fails in the environment, skip the rest of the suite.
    let policy = SandboxPolicy::new_workspace_write_policy();
    let probe = "__codex_seatbelt_preflight__";
    let ok = touch(probe, cwd, &policy).await;
    if ok {
        let _ = std::fs::remove_file(cwd.join(probe));
    }
    ok
}

async fn touch(path: &str, cwd: &Path, policy: &SandboxPolicy) -> bool {
    let mut child = spawn_command_under_seatbelt(
        vec!["/usr/bin/touch".to_string(), path.to_string()],
        policy,
        cwd.to_path_buf(),
        StdioPolicy::RedirectForShellTool,
        HashMap::new(),
    )
    .await
    .unwrap();
    child.wait().await.unwrap().success()
}

async fn touch_abs(path: &Path, cwd: &Path, policy: &SandboxPolicy) -> bool {
    let mut child = spawn_command_under_seatbelt(
        vec![
            "/usr/bin/touch".to_string(),
            path.to_string_lossy().to_string(),
        ],
        policy,
        cwd.to_path_buf(),
        StdioPolicy::RedirectForShellTool,
        HashMap::new(),
    )
    .await
    .unwrap();
    child.wait().await.unwrap().success()
}
