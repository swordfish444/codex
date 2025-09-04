use serde::Deserialize;
use serde::Serialize;
use shlex;
use std::path::Path;
use std::path::PathBuf;
use tracing::trace;
use uuid::Uuid;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct ZshShell {
    pub(crate) shell_path: String,
    pub(crate) zshrc_path: String,
    pub(crate) shell_snapshot: Option<ShellSnapshot>,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct ShellSnapshot {
    pub(crate) path: PathBuf,
}

impl ShellSnapshot {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct PowerShellConfig {
    exe: String, // Executable name or path, e.g. "pwsh" or "powershell.exe".
    bash_exe_fallback: Option<PathBuf>, // In case the model generates a bash command.
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum Shell {
    Zsh(ZshShell),
    PowerShell(PowerShellConfig),
    Unknown,
}

impl Shell {
    pub fn format_default_shell_invocation(&self, command: Vec<String>) -> Option<Vec<String>> {
        match self {
            Shell::Zsh(zsh) => {
                if !Path::new(&zsh.zshrc_path).exists() {
                    return None;
                }

                let joined = strip_bash_lc(&command)
                    .or_else(|| shlex::try_join(command.iter().map(|s| s.as_str())).ok());

                let joined = joined?;

                if let Some(shell_snapshot) = &zsh.shell_snapshot
                    && shell_snapshot.path.exists()
                {
                    let snapshot_path_string = shell_snapshot.path.to_string_lossy();
                    trace!(
                        snapshot_path = %snapshot_path_string,
                        "using cached zsh snapshot"
                    );
                    return Some(vec![
                        zsh.shell_path.clone(),
                        "-c".to_string(),
                        format!("source {snapshot_path_string} && ({joined})"),
                    ]);
                }

                trace!("no snapshot available; falling back to zshrc");

                let zshrc_path = &zsh.zshrc_path;
                Some(vec![
                    zsh.shell_path.clone(),
                    "-lc".to_string(),
                    format!("source {zshrc_path} && ({joined})"),
                ])
            }
            Shell::PowerShell(ps) => {
                // If model generated a bash command, prefer a detected bash fallback
                if let Some(script) = strip_bash_lc(&command) {
                    return match &ps.bash_exe_fallback {
                        Some(bash) => Some(vec![
                            bash.to_string_lossy().to_string(),
                            "-lc".to_string(),
                            script,
                        ]),

                        // No bash fallback → run the script under PowerShell.
                        // It will likely fail (except for some simple commands), but the error
                        // should give a clue to the model to fix upon retry that it's running under PowerShell.
                        None => Some(vec![
                            ps.exe.clone(),
                            "-NoProfile".to_string(),
                            "-Command".to_string(),
                            script,
                        ]),
                    };
                }

                // Not a bash command. If model did not generate a PowerShell command,
                // turn it into a PowerShell command.
                let first = command.first().map(String::as_str);
                if first != Some(ps.exe.as_str()) {
                    // TODO (CODEX_2900): Handle escaping newlines.
                    if command.iter().any(|a| a.contains('\n') || a.contains('\r')) {
                        return Some(command);
                    }

                    let joined = shlex::try_join(command.iter().map(|s| s.as_str())).ok();
                    return joined.map(|arg| {
                        vec![
                            ps.exe.clone(),
                            "-NoProfile".to_string(),
                            "-Command".to_string(),
                            arg,
                        ]
                    });
                }

                // Model generated a PowerShell command. Run it.
                Some(command)
            }
            Shell::Unknown => None,
        }
    }

    pub fn name(&self) -> Option<String> {
        match self {
            Shell::Zsh(zsh) => std::path::Path::new(&zsh.shell_path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string()),
            Shell::PowerShell(ps) => Some(ps.exe.clone()),
            Shell::Unknown => None,
        }
    }
}

fn strip_bash_lc(command: &Vec<String>) -> Option<String> {
    match command.as_slice() {
        // exactly three items
        [first, second, third]
            // first two must be "bash", "-lc"
            if first == "bash" && second == "-lc" =>
        {
            Some(third.clone())
        }
        _ => None,
    }
}

#[cfg(target_os = "macos")]
pub async fn default_user_shell(session_id: Uuid) -> Shell {
    let user = whoami::username();
    let home = PathBuf::from(format!("/Users/{user}"));
    let output = tokio::process::Command::new("dscl")
        .args([".", "-read", home.to_string_lossy().as_ref(), "UserShell"])
        .output()
        .await
        .ok();
    match output {
        Some(o) => {
            if !o.status.success() {
                return Shell::Unknown;
            }
            let stdout = String::from_utf8_lossy(&o.stdout);
            for line in stdout.lines() {
                if let Some(shell_path) = line.strip_prefix("UserShell: ")
                    && shell_path.ends_with("/zsh")
                {
                    let snapshot_path = ensure_zsh_snapshot(shell_path, &home, session_id).await;
                    if snapshot_path.is_none() {
                        trace!("failed to prepare zsh snapshot; using live profile");
                    }

                    return Shell::Zsh(ZshShell {
                        shell_path: shell_path.to_string(),
                        zshrc_path: home.join(".zshrc").to_string_lossy().to_string(),
                        shell_snapshot: snapshot_path.map(ShellSnapshot::new),
                    });
                }
            }

            Shell::Unknown
        }
        _ => Shell::Unknown,
    }
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
pub async fn default_user_shell(_session_id: Uuid) -> Shell {
    Shell::Unknown
}

#[cfg(target_os = "windows")]
pub async fn default_user_shell(_session_id: Uuid) -> Shell {
    use tokio::process::Command;

    // Prefer PowerShell 7+ (`pwsh`) if available, otherwise fall back to Windows PowerShell.
    let has_pwsh = Command::new("pwsh")
        .arg("-NoLogo")
        .arg("-NoProfile")
        .arg("-Command")
        .arg("$PSVersionTable.PSVersion.Major")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);
    let bash_exe = if Command::new("bash.exe")
        .arg("--version")
        .output()
        .await
        .ok()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        which::which("bash.exe").ok()
    } else {
        None
    };

    if has_pwsh {
        Shell::PowerShell(PowerShellConfig {
            exe: "pwsh.exe".to_string(),
            bash_exe_fallback: bash_exe,
        })
    } else {
        Shell::PowerShell(PowerShellConfig {
            exe: "powershell.exe".to_string(),
            bash_exe_fallback: bash_exe,
        })
    }
}

#[cfg(target_os = "macos")]
fn zsh_profile_paths(home: &Path) -> Vec<PathBuf> {
    [".zshenv", ".zprofile", ".zshrc", ".zlogin"]
        .into_iter()
        .map(|name| home.join(name))
        .collect()
}

#[cfg(target_os = "macos")]
fn zsh_profile_source_script(home: &Path) -> String {
    zsh_profile_paths(home)
        .into_iter()
        .map(|profile| {
            let profile_string = profile.to_string_lossy().into_owned();
            let quoted = shlex::try_quote(&profile_string)
                .map(|cow| cow.into_owned())
                .unwrap_or(profile_string.clone());

            format!("[ -f {quoted} ] && source {quoted}")
        })
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(target_os = "macos")]
async fn ensure_zsh_snapshot(shell_path: &str, home: &Path, session_id: Uuid) -> Option<PathBuf> {
    let snapshot_path = home
        .join(".codex")
        .join(format!("codex_shell_snapshot_{session_id}.zsh"));

    // Check if an update in the profile requires to re-generate the snapshot.
    let snapshot_is_stale = async {
        let snapshot_metadata = tokio::fs::metadata(&snapshot_path).await.ok()?;
        let snapshot_modified = snapshot_metadata.modified().ok()?;

        for profile in zsh_profile_paths(home) {
            let Ok(profile_metadata) = tokio::fs::metadata(&profile).await else {
                continue;
            };

            let Ok(profile_modified) = profile_metadata.modified() else {
                return Some(true);
            };

            if profile_modified > snapshot_modified {
                return Some(true);
            }
        }

        Some(false)
    }
    .await
    .unwrap_or(true);

    if !snapshot_is_stale {
        return Some(snapshot_path);
    }

    match regenerate_zsh_snapshot(shell_path, home, &snapshot_path).await {
        Ok(()) => Some(snapshot_path),
        Err(err) => {
            tracing::warn!("failed to generate zsh snapshot: {err}");
            None
        }
    }
}

#[cfg(target_os = "macos")]
async fn regenerate_zsh_snapshot(
    shell_path: &str,
    home: &Path,
    snapshot_path: &Path,
) -> std::io::Result<()> {
    // Use `emulate -L sh` instead of `set -o posix` so we work on zsh builds
    // that disable that option. Guard `alias -p` with `|| true` so the script
    // keeps a zero exit status even if aliases are disabled.
    let mut capture_script = String::new();
    let profile_sources = zsh_profile_source_script(home);
    if !profile_sources.is_empty() {
        capture_script.push_str(&format!("{profile_sources}; "));
    }

    let zshrc = home.join(".zshrc");

    capture_script.push_str(
        &format!("source {}/.zshrc; setopt posixbuiltins; export -p; {{ alias | sed 's/^/alias /'; }} 2>/dev/null || true", zshrc.display()),
    );
    let output = tokio::process::Command::new(shell_path)
        .arg("-lc")
        .arg(capture_script)
        .env("HOME", home)
        .output()
        .await?;

    if !output.status.success() {
        return Err(std::io::Error::other(format!(
            "snapshot capture exited with status {}",
            output.status
        )));
    }

    let mut contents = String::from("# Generated by Codex. Do not edit.\n");

    contents.push_str(&String::from_utf8_lossy(&output.stdout));
    contents.push('\n');

    if let Some(parent) = snapshot_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let tmp_path = snapshot_path.with_extension("tmp");
    tokio::fs::write(&tmp_path, contents).await?;

    #[cfg(unix)]
    {
        // Restrict the snapshot to user read/write so that environment variables or aliases
        // that may contain secrets are not exposed to other users on the system.
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::Permissions::from_mode(0o600);
        tokio::fs::set_permissions(&tmp_path, permissions).await?;
    }

    tokio::fs::rename(&tmp_path, snapshot_path).await?;
    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn delete_shell_snapshot(path: &Path) {
    if let Err(err) = std::fs::remove_file(path) {
        trace!(?path, %err, "failed to delete shell snapshot");
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn delete_shell_snapshot(_path: &Path) {}

#[cfg(test)]
#[cfg(target_os = "macos")]
mod tests {
    use super::*;
    use std::path::Path;
    use std::process::Command;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_current_shell_detects_zsh() {
        let shell = Command::new("sh")
            .arg("-c")
            .arg("echo $SHELL")
            .output()
            .unwrap();

        let home = std::env::var("HOME").unwrap();
        let shell_path = String::from_utf8_lossy(&shell.stdout).trim().to_string();
        if shell_path.ends_with("/zsh") {
            match default_user_shell(Uuid::new_v4()).await {
                Shell::Zsh(zsh) => {
                    assert_eq!(zsh.shell_path, shell_path);
                    assert_eq!(zsh.zshrc_path, format!("{home}/.zshrc"));
                }
                other => panic!("unexpected shell returned: {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn test_run_with_profile_zshrc_not_exists() {
        let shell = Shell::Zsh(ZshShell {
            shell_path: "/bin/zsh".to_string(),
            zshrc_path: "/does/not/exist/.zshrc".to_string(),
            shell_snapshot: None,
        });
        let actual_cmd = shell.format_default_shell_invocation(vec!["myecho".to_string()]);
        assert_eq!(actual_cmd, None);
    }

    #[tokio::test]
    async fn test_snapshot_generation_uses_session_id_and_cleanup() {
        let shell_path = "/bin/zsh";
        if !Path::new(shell_path).exists() {
            return;
        }

        let temp_home = tempfile::tempdir().unwrap();
        std::fs::write(
            temp_home.path().join(".zshrc"),
            "export SNAPSHOT_TEST_VAR=1\nalias snapshot_test_alias='echo hi'\n",
        )
        .unwrap();

        let session_id = Uuid::new_v4();
        let snapshot_path = ensure_zsh_snapshot(shell_path, temp_home.path(), session_id)
            .await
            .expect("snapshot path");

        let filename = snapshot_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert!(filename.contains(&session_id.to_string()));
        assert!(snapshot_path.exists());

        let snapshot_path_second = ensure_zsh_snapshot(shell_path, temp_home.path(), session_id)
            .await
            .expect("snapshot path");
        assert_eq!(snapshot_path, snapshot_path_second);

        let contents = std::fs::read_to_string(&snapshot_path).unwrap();
        assert!(contents.contains("alias snapshot_test_alias='echo hi'"));
        assert!(contents.contains("SNAPSHOT_TEST_VAR=1"));

        delete_shell_snapshot(&snapshot_path);
        assert!(!snapshot_path.exists());
    }

    #[test]
    fn format_default_shell_invocation_prefers_snapshot_when_available() {
        let temp_dir = tempfile::tempdir().unwrap();
        let snapshot_path = temp_dir.path().join("snapshot.zsh");
        std::fs::write(&snapshot_path, "export SNAPSHOT_READY=1").unwrap();

        let shell = Shell::Zsh(ZshShell {
            shell_path: "/bin/zsh".to_string(),
            zshrc_path: {
                let path = temp_dir.path().join(".zshrc");
                std::fs::write(&path, "# test zshrc").unwrap();
                path.to_string_lossy().to_string()
            },
            shell_snapshot: Some(ShellSnapshot::new(snapshot_path.clone())),
        });

        let invocation = shell.format_default_shell_invocation(vec!["echo".to_string()]);
        let expected_command = vec!["/bin/zsh".to_string(), "-c".to_string(), {
            let snapshot_path = snapshot_path.to_string_lossy();
            format!("source {snapshot_path} && (echo)")
        }];

        assert_eq!(invocation, Some(expected_command));
    }

    #[tokio::test]
    async fn test_run_with_profile_escaping_and_execution() {
        let shell_path = "/bin/zsh";

        let cases = vec![
            (
                vec!["myecho"],
                vec![shell_path, "-lc", "source ZSHRC_PATH && (myecho)"],
                Some("It works!\n"),
            ),
            (
                vec!["myecho"],
                vec![shell_path, "-lc", "source ZSHRC_PATH && (myecho)"],
                Some("It works!\n"),
            ),
            (
                vec!["bash", "-c", "echo 'single' \"double\""],
                vec![
                    shell_path,
                    "-lc",
                    "source ZSHRC_PATH && (bash -c \"echo 'single' \\\"double\\\"\")",
                ],
                Some("single double\n"),
            ),
            (
                vec!["bash", "-lc", "echo 'single' \"double\""],
                vec![
                    shell_path,
                    "-lc",
                    "source ZSHRC_PATH && (echo 'single' \"double\")",
                ],
                Some("single double\n"),
            ),
        ];
        for (input, expected_cmd, expected_output) in cases {
            use std::collections::HashMap;
            use std::path::PathBuf;

            use crate::exec::ExecParams;
            use crate::exec::SandboxType;
            use crate::exec::process_exec_tool_call;
            use crate::protocol::SandboxPolicy;

            // create a temp directory with a zshrc file in it
            let temp_home = tempfile::tempdir().unwrap();
            let zshrc_path = temp_home.path().join(".zshrc");
            std::fs::write(
                &zshrc_path,
                r#"
                    set -x
                    function myecho {
                        echo 'It works!'
                    }
                    "#,
            )
            .unwrap();
            let shell = Shell::Zsh(ZshShell {
                shell_path: shell_path.to_string(),
                zshrc_path: zshrc_path.to_str().unwrap().to_string(),
                shell_snapshot: None,
            });

            let actual_cmd = shell
                .format_default_shell_invocation(input.iter().map(|s| s.to_string()).collect());
            let expected_cmd = expected_cmd
                .iter()
                .map(|s| {
                    s.replace("ZSHRC_PATH", zshrc_path.to_str().unwrap())
                        .to_string()
                })
                .collect();

            assert_eq!(actual_cmd, Some(expected_cmd));
            // Actually run the command and check output/exit code
            let output = process_exec_tool_call(
                ExecParams {
                    command: actual_cmd.unwrap(),
                    cwd: PathBuf::from(temp_home.path()),
                    timeout_ms: None,
                    env: HashMap::from([(
                        "HOME".to_string(),
                        temp_home.path().to_str().unwrap().to_string(),
                    )]),
                    with_escalated_permissions: None,
                    justification: None,
                },
                SandboxType::None,
                &SandboxPolicy::DangerFullAccess,
                &None,
                None,
            )
            .await
            .unwrap();

            assert_eq!(output.exit_code, 0, "input: {input:?} output: {output:?}");
            if let Some(expected) = expected_output {
                assert_eq!(
                    output.stdout.text, expected,
                    "input: {input:?} output: {output:?}"
                );
            }
        }
    }
}

#[cfg(test)]
#[cfg(target_os = "windows")]
mod tests_windows {
    use super::*;

    #[test]
    fn test_format_default_shell_invocation_powershell() {
        let cases = vec![
            (
                Shell::PowerShell(PowerShellConfig {
                    exe: "pwsh.exe".to_string(),
                    bash_exe_fallback: None,
                }),
                vec!["bash", "-lc", "echo hello"],
                vec!["pwsh.exe", "-NoProfile", "-Command", "echo hello"],
            ),
            (
                Shell::PowerShell(PowerShellConfig {
                    exe: "powershell.exe".to_string(),
                    bash_exe_fallback: None,
                }),
                vec!["bash", "-lc", "echo hello"],
                vec!["powershell.exe", "-NoProfile", "-Command", "echo hello"],
            ),
            (
                Shell::PowerShell(PowerShellConfig {
                    exe: "pwsh.exe".to_string(),
                    bash_exe_fallback: Some(PathBuf::from("bash.exe")),
                }),
                vec!["bash", "-lc", "echo hello"],
                vec!["bash.exe", "-lc", "echo hello"],
            ),
            (
                Shell::PowerShell(PowerShellConfig {
                    exe: "pwsh.exe".to_string(),
                    bash_exe_fallback: Some(PathBuf::from("bash.exe")),
                }),
                vec![
                    "bash",
                    "-lc",
                    "apply_patch <<'EOF'\n*** Begin Patch\n*** Update File: destination_file.txt\n-original content\n+modified content\n*** End Patch\nEOF",
                ],
                vec![
                    "bash.exe",
                    "-lc",
                    "apply_patch <<'EOF'\n*** Begin Patch\n*** Update File: destination_file.txt\n-original content\n+modified content\n*** End Patch\nEOF",
                ],
            ),
            (
                Shell::PowerShell(PowerShellConfig {
                    exe: "pwsh.exe".to_string(),
                    bash_exe_fallback: Some(PathBuf::from("bash.exe")),
                }),
                vec!["echo", "hello"],
                vec!["pwsh.exe", "-NoProfile", "-Command", "echo hello"],
            ),
            (
                Shell::PowerShell(PowerShellConfig {
                    exe: "pwsh.exe".to_string(),
                    bash_exe_fallback: Some(PathBuf::from("bash.exe")),
                }),
                vec!["pwsh.exe", "-NoProfile", "-Command", "echo hello"],
                vec!["pwsh.exe", "-NoProfile", "-Command", "echo hello"],
            ),
            (
                // TODO (CODEX_2900): Handle escaping newlines for powershell invocation.
                Shell::PowerShell(PowerShellConfig {
                    exe: "powershell.exe".to_string(),
                    bash_exe_fallback: Some(PathBuf::from("bash.exe")),
                }),
                vec![
                    "codex-mcp-server.exe",
                    "--codex-run-as-apply-patch",
                    "*** Begin Patch\n*** Update File: C:\\Users\\person\\destination_file.txt\n-original content\n+modified content\n*** End Patch",
                ],
                vec![
                    "codex-mcp-server.exe",
                    "--codex-run-as-apply-patch",
                    "*** Begin Patch\n*** Update File: C:\\Users\\person\\destination_file.txt\n-original content\n+modified content\n*** End Patch",
                ],
            ),
        ];

        for (shell, input, expected_cmd) in cases {
            let actual_cmd = shell
                .format_default_shell_invocation(input.iter().map(|s| s.to_string()).collect());
            assert_eq!(
                actual_cmd,
                Some(expected_cmd.iter().map(|s| s.to_string()).collect())
            );
        }
    }
}
