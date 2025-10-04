use codex_common::CliConfigOverrides;

#[cfg(windows)]
use anyhow::Context;
#[cfg(windows)]
use anyhow::anyhow;
#[cfg(windows)]
use codex_core::config::Config;
#[cfg(windows)]
use codex_core::config::ConfigOverrides;
#[cfg(windows)]
use std::env;
#[cfg(windows)]
use std::ffi::OsString;
#[cfg(windows)]
use std::fs;
#[cfg(windows)]
use std::path::Component;
#[cfg(windows)]
use std::path::Path;
#[cfg(windows)]
use std::path::PathBuf;
#[cfg(windows)]
use std::path::Prefix;
#[cfg(windows)]
use std::process::Command;
#[cfg(windows)]
use std::process::Stdio;

pub async fn maybe_relaunch_in_wsl(overrides: &CliConfigOverrides) -> anyhow::Result<()> {
    maybe_relaunch_in_wsl_inner(overrides).await
}

#[cfg(not(windows))]
async fn maybe_relaunch_in_wsl_inner(_overrides: &CliConfigOverrides) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(windows)]
async fn maybe_relaunch_in_wsl_inner(overrides: &CliConfigOverrides) -> anyhow::Result<()> {
    const SKIP_ENV: &str = "CODEX_SKIP_WSL_REDIRECT";

    if env::var_os(SKIP_ENV).is_some() || running_inside_wsl() {
        return Ok(());
    }

    let cli_overrides = overrides
        .parse_overrides()
        .map_err(|err| anyhow!("{err}"))?;

    let config = Config::load_with_cli_overrides(cli_overrides, ConfigOverrides::default())
        .await
        .context("failed to load Codex config while evaluating windows.prefer_wsl")?;

    let windows_cfg = config.windows;
    if !windows_cfg.prefer_wsl {
        return Ok(());
    }

    if !is_wsl_available() {
        if !windows_cfg.hide_wsl_notice {
            eprintln!("WSL not detected; continuing to run Codex on Windows");
        }
        return Ok(());
    }

    let current_dir = env::current_dir().context("failed to resolve current directory")?;
    let wsl_cwd = match windows_path_to_wsl(&current_dir) {
        Some(path) => path,
        None => {
            if !windows_cfg.hide_wsl_notice {
                eprintln!(
                    "Unable to map current directory {} into WSL; continuing on Windows",
                    current_dir.display()
                );
            }
            return Ok(());
        }
    };

    let linux_binary = locate_linux_binary_path();
    let (command_choice, notice_suffix) = match linux_binary {
        Some(ref binary_path) => match windows_path_to_wsl(binary_path) {
            Some(binary_wsl) => (
                CommandChoice::Binary(binary_wsl),
                "the bundled Linux binary",
            ),
            None => {
                if !windows_cfg.hide_wsl_notice {
                    eprintln!(
                        "Unable to translate Codex Linux binary path {} into WSL; continuing on Windows",
                        binary_path.display()
                    );
                }
                (CommandChoice::Fallback, "codex from WSL PATH")
            }
        },
        None => (CommandChoice::Fallback, "codex from WSL PATH"),
    };

    if !windows_cfg.hide_wsl_notice {
        eprintln!(
            "windows.prefer_wsl is enabled; re-launching Codex inside WSL using {notice_suffix}"
        );
    }

    let args: Vec<OsString> = env::args_os().skip(1).collect();

    let mut cmd = Command::new("wsl.exe");
    cmd.arg("--cd").arg(&wsl_cwd);
    match &command_choice {
        CommandChoice::Binary(binary) => {
            cmd.arg("--exec").arg(binary);
        }
        CommandChoice::Fallback => {
            cmd.arg("--exec").arg("codex");
        }
    }
    cmd.env(SKIP_ENV, "1");
    cmd.args(args);
    cmd.stdin(Stdio::inherit());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());

    let status = cmd.status().context("failed to invoke wsl.exe")?;
    if let Some(code) = status.code() {
        std::process::exit(code);
    }
    std::process::exit(1);
}

#[cfg(windows)]
#[derive(Clone, Debug)]
enum CommandChoice {
    Binary(String),
    Fallback,
}

#[cfg(windows)]
fn running_inside_wsl() -> bool {
    env::var_os("WSL_DISTRO_NAME").is_some() || env::var_os("WSL_INTEROP").is_some()
}

#[cfg(windows)]
fn is_wsl_available() -> bool {
    Command::new("wsl.exe")
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn locate_linux_binary_path() -> Option<PathBuf> {
    let exe = env::current_exe().ok()?;
    let vendor_dir = locate_vendor_root(&exe)?;
    let linux_target = match env::consts::ARCH {
        "x86_64" => "x86_64-unknown-linux-musl",
        "aarch64" => "aarch64-unknown-linux-musl",
        _ => return None,
    };
    let binary_path = vendor_dir.join(linux_target).join("codex").join("codex");
    if binary_path.exists() {
        Some(binary_path)
    } else {
        None
    }
}

#[cfg(windows)]
fn locate_vendor_root(exe_path: &Path) -> Option<PathBuf> {
    let mut current = exe_path.parent()?;
    for _ in 0..5 {
        if current.file_name().and_then(|s| s.to_str()) == Some("vendor") {
            return Some(current.to_path_buf());
        }
        current = current.parent()?;
    }
    None
}

#[cfg(windows)]
fn windows_path_to_wsl(path: &Path) -> Option<String> {
    use std::borrow::Cow;

    let absolute = if path.is_absolute() {
        Cow::Borrowed(path)
    } else {
        Cow::Owned(fs::canonicalize(path).ok()?)
    };

    let mut components = absolute.components();
    let prefix = match components.next()? {
        Component::Prefix(prefix) => prefix.kind(),
        _ => return None,
    };

    let drive_letter = match prefix {
        Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => letter as char,
        _ => return None,
    };

    let mut parts = Vec::new();
    for component in components {
        match component {
            Component::RootDir => {}
            Component::Normal(segment) => {
                parts.push(segment.to_string_lossy().to_string());
            }
            Component::CurDir => {}
            Component::ParentDir => return None,
            Component::Prefix(_) => return None,
        }
    }

    let drive = drive_letter.to_ascii_lowercase();
    let mut wsl_path = format!("/mnt/{drive}");
    if !parts.is_empty() {
        wsl_path.push('/');
        wsl_path.push_str(&parts.join("/"));
    }
    Some(wsl_path)
}
