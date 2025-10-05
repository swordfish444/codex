use codex_common::CliConfigOverrides;

#[cfg(windows)]
mod windows_only {
    use super::*;
    use anyhow::Context;
    use anyhow::anyhow;
    use codex_core::config::Config;
    use codex_core::config::ConfigOverrides;
    use std::borrow::Cow;
    use std::env;
    use std::ffi::OsString;
    use std::fs;
    use std::path::Component;
    use std::path::Path;
    use std::path::PathBuf;
    use std::path::Prefix;
    use std::process::Command;
    use std::process::Stdio;

    pub(super) async fn maybe_relaunch_in_wsl_inner(
        overrides: &CliConfigOverrides,
    ) -> anyhow::Result<()> {
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

        let codex_home_override = env::var_os("CODEX_HOME").map(PathBuf::from);
        let codex_home_display = codex_home_override.as_ref().unwrap_or(&config.codex_home);
        let codex_home_wsl = codex_home_override
            .as_ref()
            .and_then(windows_path_to_wsl)
            .or_else(|| windows_path_to_wsl(&config.codex_home));
        let codex_home_wsl = match codex_home_wsl {
            Some(path) => path,
            None => {
                if !windows_cfg.hide_wsl_notice {
                    eprintln!(
                        "Unable to translate Codex home {} into WSL; continuing on Windows",
                        codex_home_display.display()
                    );
                }
                return Ok(());
            }
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
        cmd.env("CODEX_HOME", OsString::from(&codex_home_wsl));
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

    #[derive(Clone, Debug)]
    enum CommandChoice {
        Binary(String),
        Fallback,
    }

    fn running_inside_wsl() -> bool {
        env::var_os("WSL_DISTRO_NAME").is_some() || env::var_os("WSL_INTEROP").is_some()
    }

    fn is_wsl_available() -> bool {
        Command::new("wsl.exe")
            .arg("--help")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

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

    fn windows_path_to_wsl(path: &Path) -> Option<String> {
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
}

#[cfg(windows)]
use windows_only::maybe_relaunch_in_wsl_inner;

#[cfg(not(windows))]
async fn maybe_relaunch_in_wsl_inner(_overrides: &CliConfigOverrides) -> anyhow::Result<()> {
    Ok(())
}

pub async fn maybe_relaunch_in_wsl(overrides: &CliConfigOverrides) -> anyhow::Result<()> {
    maybe_relaunch_in_wsl_inner(overrides).await
}
