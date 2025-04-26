//! Cross-platform helper to spawn a detached `codex-exec` agent.

use crate::store::Paths;
use anyhow::{Context, Result};
use std::fs::OpenOptions;
use tokio::process::{Child, Command};

#[cfg(unix)]
pub fn spawn_agent(exec: &str, id: &str, paths: &Paths, kill_on_drop: bool) -> Result<Child> {
use std::io;

    // Prepare stdio handles first.
    let stdin = OpenOptions::new().read(true).open("/dev/null")?;
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&paths.stdout)?;
    let stderr = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&paths.stderr)?;

    let mut cmd = Command::new(exec);
    cmd.arg("--job").arg(id)
        .stdin(stdin)
        .stdout(stdout)
        .stderr(stderr);

    if kill_on_drop {
        cmd.kill_on_drop(true);
    }

    // Detach: make a new session and ignore SIGHUP.
    unsafe {
        cmd.pre_exec(|| {
            unsafe {
                // setsid(2)
                if libc::setsid() == -1 {
                    return Err(io::Error::last_os_error());
                }
                libc::signal(libc::SIGHUP, libc::SIG_IGN);
            }
            Ok(())
        });
    }

    let child = cmd.spawn().context("failed to spawn agent")?;
    Ok(child)
}

#[cfg(windows)]
pub fn spawn_agent(exec: &str, id: &str, paths: &Paths, kill_on_drop: bool) -> Result<Child> {
    use std::os::windows::process::CommandExt;

    const DETACHED_PROCESS: u32 = 0x00000008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;

    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&paths.stdout)?;
    let stderr = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&paths.stderr)?;

    let mut cmd = Command::new(exec);
    cmd.arg("--job").arg(id)
        .stdin(std::process::Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);

    if kill_on_drop {
        cmd.kill_on_drop(true);
    }

    let child = cmd.spawn().context("failed to spawn agent")?;
    Ok(child)
}
