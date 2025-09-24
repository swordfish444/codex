#![cfg(target_os = "windows")]

use std::collections::HashMap;

use assert_cmd::cargo::cargo_bin;
use codex_core::protocol::SandboxPolicy;
use codex_core::spawn::StdioPolicy;
use codex_core::windows_sandbox::spawn_command_under_windows_sandbox;
use tempfile::tempdir;
use tokio::fs::File;
use tokio::fs::create_dir_all;
use tokio::fs::try_exists;
use tokio::io::AsyncReadExt;

fn windows_shell_command(content: &str, path: &std::path::Path) -> Vec<String> {
    vec![
        "cmd.exe".to_string(),
        "/C".to_string(),
        format!("{content}>\"{}\"", path.display()),
    ]
}

fn inherit_env() -> HashMap<String, String> {
    std::env::vars().collect()
}

#[tokio::test]
async fn sandbox_denies_writes_outside_policy_cwd() {
    let temp = tempdir().expect("create temp dir");
    let sandbox_root = temp.path().join("sandbox");
    let command_root = temp.path().join("command");
    create_dir_all(&sandbox_root)
        .await
        .expect("mkdir sandbox root");
    create_dir_all(&command_root)
        .await
        .expect("mkdir command root");

    let canonical_sandbox_root = tokio::fs::canonicalize(&sandbox_root)
        .await
        .expect("canonicalize sandbox root");
    let allowed_path = canonical_sandbox_root.join("allowed.txt");
    let disallowed_path = command_root.join("forbidden.txt");

    let policy = SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![],
        network_access: false,
        exclude_tmpdir_env_var: true,
        exclude_slash_tmp: true,
    };

    let exe = cargo_bin("codex-windows-sandbox");

    let mut child = spawn_command_under_windows_sandbox(
        &exe,
        windows_shell_command("echo forbidden", &disallowed_path),
        command_root.clone(),
        &policy,
        canonical_sandbox_root.as_path(),
        StdioPolicy::Inherit,
        inherit_env(),
    )
    .await
    .expect("spawn forbidden command");
    let status = child
        .wait()
        .await
        .expect("wait for forbidden command to exit");
    assert!(
        !status.success(),
        "sandbox unexpectedly allowed writing outside policy cwd: {status:?}"
    );
    assert!(
        !try_exists(&disallowed_path)
            .await
            .expect("try_exists forbidden")
    );

    let mut child = spawn_command_under_windows_sandbox(
        &exe,
        windows_shell_command("echo allowed", &allowed_path),
        command_root.clone(),
        &policy,
        canonical_sandbox_root.as_path(),
        StdioPolicy::Inherit,
        inherit_env(),
    )
    .await
    .expect("spawn allowed command");
    let status = child
        .wait()
        .await
        .expect("wait for allowed command to exit");
    assert!(status.success(), "allowed write should succeed: {status:?}");
    assert!(try_exists(&allowed_path).await.expect("try_exists allowed"));
}

#[tokio::test]
async fn sandbox_blocks_git_directory_writes() {
    let temp = tempdir().expect("create temp dir");
    let sandbox_root = temp.path().join("sandbox");
    create_dir_all(&sandbox_root)
        .await
        .expect("mkdir sandbox root");
    let git_dir = sandbox_root.join(".git");
    create_dir_all(&git_dir).await.expect("mkdir git dir");

    let canonical_sandbox_root = tokio::fs::canonicalize(&sandbox_root)
        .await
        .expect("canonicalize sandbox root");
    let git_file = git_dir.join("config");

    let policy = SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![],
        network_access: false,
        exclude_tmpdir_env_var: true,
        exclude_slash_tmp: true,
    };

    let exe = cargo_bin("codex-windows-sandbox");

    let mut child = spawn_command_under_windows_sandbox(
        &exe,
        windows_shell_command("echo blocked", &git_file),
        canonical_sandbox_root.clone(),
        &policy,
        canonical_sandbox_root.as_path(),
        StdioPolicy::Inherit,
        inherit_env(),
    )
    .await
    .expect("spawn git write");
    let status = child.wait().await.expect("wait for git write");
    assert!(
        !status.success(),
        "sandbox unexpectedly allowed writing inside .git: {status:?}"
    );
    assert!(!try_exists(&git_file).await.expect("try_exists git"));

    let mut child = spawn_command_under_windows_sandbox(
        &exe,
        windows_shell_command("echo ok", &canonical_sandbox_root.join("user.txt")),
        canonical_sandbox_root.clone(),
        &policy,
        canonical_sandbox_root.as_path(),
        StdioPolicy::Inherit,
        inherit_env(),
    )
    .await
    .expect("spawn allowed write inside sandbox");
    let status = child
        .wait()
        .await
        .expect("wait for allowed git-adjacent write");
    assert!(status.success(), "expected success writing outside .git");
    let mut file = File::open(canonical_sandbox_root.join("user.txt"))
        .await
        .expect("open user file");
    let mut buf = String::new();
    file.read_to_string(&mut buf).await.expect("read user file");
    assert!(buf.contains("ok"));
}
