#[cfg(target_os = "windows")]
use crate::config_types::SandboxWorkspaceWrite;
use crate::config_types::WindowsConfig;
use crate::exec::SandboxType;
use crate::protocol::SandboxPolicy;
use crate::spawn::StdioPolicy;
#[cfg(target_os = "windows")]
use crate::spawn::spawn_child_async;
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use tokio::process::Child;

#[cfg(target_os = "windows")]
use std::process::Command as StdCommand;
#[cfg(target_os = "windows")]
use std::sync::OnceLock;
#[cfg(target_os = "windows")]
use std::sync::RwLock;
#[cfg(target_os = "windows")]
use std::sync::atomic::AtomicBool;
#[cfg(target_os = "windows")]
use std::sync::atomic::Ordering;

#[cfg(target_os = "windows")]
static WINDOWS_SETTINGS: RwLock<WindowsConfig> = RwLock::new(WindowsConfig::default());
#[cfg(target_os = "windows")]
static NOTICE_PRINTED: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "windows")]
static WSL_AVAILABLE: OnceLock<bool> = OnceLock::new();

pub fn update_settings(settings: WindowsConfig) {
    #[cfg(target_os = "windows")]
    {
        if let Ok(mut guard) = WINDOWS_SETTINGS.write() {
            *guard = settings;
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = settings;
    }
}

pub fn preferred_sandbox() -> Option<SandboxType> {
    if prefer_wsl() {
        Some(SandboxType::WindowsWsl)
    } else {
        None
    }
}

pub fn prefer_wsl() -> bool {
    #[cfg(target_os = "windows")]
    {
        let settings = current_settings();
        settings.prefer_wsl && is_wsl_available()
    }

    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub fn hide_wsl_notice() -> bool {
    #[cfg(target_os = "windows")]
    {
        current_settings().hide_wsl_notice
    }

    #[cfg(not(target_os = "windows"))]
    {
        true
    }
}

#[cfg(target_os = "windows")]
fn current_settings() -> WindowsConfig {
    WINDOWS_SETTINGS
        .read()
        .map(|guard| guard.clone())
        .unwrap_or_default()
}

#[cfg(target_os = "windows")]
fn is_wsl_available() -> bool {
    *WSL_AVAILABLE.get_or_init(|| {
        StdCommand::new("wsl")
            .arg("--status")
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    })
}

#[cfg(target_os = "windows")]
fn maybe_print_notice() {
    if hide_wsl_notice() {
        return;
    }
    if !NOTICE_PRINTED.swap(true, Ordering::SeqCst) {
        eprintln!(
            "Detected prefer_wsl=true; relaunching command inside WSL for sandboxed execution."
        );
    }
}

#[cfg(target_os = "windows")]
fn sandbox_overrides_for_wsl(sandbox_policy: &SandboxPolicy) -> io::Result<Vec<String>> {
    let mut overrides = Vec::new();
    match sandbox_policy {
        SandboxPolicy::DangerFullAccess => {
            overrides.push("sandbox_mode=\"danger-full-access\"".to_string());
        }
        SandboxPolicy::ReadOnly => {
            overrides.push("sandbox_mode=\"read-only\"".to_string());
        }
        SandboxPolicy::WorkspaceWrite {
            writable_roots,
            network_access,
            exclude_tmpdir_env_var,
            exclude_slash_tmp,
        } => {
            overrides.push("sandbox_mode=\"workspace-write\"".to_string());
            let mut workspace = SandboxWorkspaceWrite {
                writable_roots: Vec::new(),
                network_access: *network_access,
                exclude_tmpdir_env_var: *exclude_tmpdir_env_var,
                exclude_slash_tmp: *exclude_slash_tmp,
            };
            for root in writable_roots {
                if let Some(converted) = convert_path_to_wsl(root) {
                    workspace.writable_roots.push(PathBuf::from(converted));
                }
            }
            let json = serde_json::to_string(&workspace).map_err(|err| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("failed to serialize sandbox workspace: {err}"),
                )
            })?;
            overrides.push(format!("sandbox_workspace_write={json}"));
        }
    }
    Ok(overrides)
}

#[cfg(target_os = "windows")]
fn convert_path_to_wsl(path: &Path) -> Option<String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map(|cwd| cwd.join(path)).ok()?
    };
    let path_str = absolute.to_string_lossy().to_string();
    run_wslpath(&path_str)
        .or_else(|| drive_path_to_wsl(&path_str))
        .map(|converted| converted.trim().to_string())
}

#[cfg(target_os = "windows")]
fn run_wslpath(path: &str) -> Option<String> {
    let output = StdCommand::new("wsl")
        .arg("wslpath")
        .arg("-a")
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

#[cfg(target_os = "windows")]
fn drive_path_to_wsl(path: &str) -> Option<String> {
    let mut chars = path.chars();
    let drive = chars.next()?;
    if chars.next()? != ':' {
        return None;
    }
    let mut rest = chars.collect::<String>();
    rest = rest.replace("\\", "/");
    Some(format!(
        "/mnt/{}/{}",
        drive.to_ascii_lowercase(),
        rest.trim_start_matches('/')
    ))
}

#[cfg(target_os = "windows")]
pub async fn spawn_command_under_windows_wsl(
    command: Vec<String>,
    command_cwd: PathBuf,
    sandbox_policy: &SandboxPolicy,
    _sandbox_policy_cwd: &Path,
    stdio_policy: StdioPolicy,
    env: HashMap<String, String>,
) -> io::Result<Child> {
    if !is_wsl_available() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "WSL is not available on this system",
        ));
    }
    maybe_print_notice();
    let mut args: Vec<String> = Vec::new();
    if let Some(cd_path) = convert_path_to_wsl(&command_cwd) {
        args.push("--cd".to_string());
        args.push(cd_path);
    }
    args.push("-e".to_string());
    args.push("codex".to_string());
    args.push("debug".to_string());
    args.push("landlock".to_string());
    for override_value in sandbox_overrides_for_wsl(sandbox_policy)? {
        args.push("-c".to_string());
        args.push(override_value);
    }
    args.push("--".to_string());
    args.extend(command);
    spawn_child_async(
        PathBuf::from("wsl.exe"),
        args,
        None,
        command_cwd,
        sandbox_policy,
        stdio_policy,
        env,
    )
    .await
}

#[cfg(not(target_os = "windows"))]
pub async fn spawn_command_under_windows_wsl(
    _command: Vec<String>,
    _command_cwd: PathBuf,
    _sandbox_policy: &SandboxPolicy,
    _sandbox_policy_cwd: &Path,
    _stdio_policy: StdioPolicy,
    _env: HashMap<String, String>,
) -> io::Result<Child> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Windows WSL sandboxing is only available on Windows",
    ))
}
