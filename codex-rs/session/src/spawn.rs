//! Cross-platform helper to spawn a fully-detached `codex-exec` process.

use crate::store::Paths;
use anyhow::{Context, Result};
use std::fs::OpenOptions;
use tokio::process::{Child, Command};

/// Spawn `codex-exec` with `exec_args`, redirecting stdio to the per-session log files and
/// detaching the process group so it survives the parent CLI.
pub fn spawn_agent(paths: &Paths, exec_args: &[String]) -> Result<Child> {
    #[cfg(unix)]
    {
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

        let mut cmd = Command::new("codex-exec");
        cmd.args(exec_args)
            .stdin(stdin)
            .stdout(stdout)
            .stderr(stderr);

        // Detach from the controlling terminal: setsid + ignore SIGHUP.
        // SAFETY: calling an `unsafe` method (`pre_exec`).  Runs in the parent process right
        // before fork; the closure then executes in the child.
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(io::Error::last_os_error());
                }
                libc::signal(libc::SIGHUP, libc::SIG_IGN);
                Ok(())
            });
        }

        let child = cmd.spawn().context("failed to spawn codex-exec")?;
        return Ok(child);
    }

    #[cfg(windows)]
    {
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

        let mut cmd = Command::new("codex-exec");
        cmd.args(exec_args)
            .stdin(std::process::Stdio::null())
            .stdout(stdout)
            .stderr(stderr)
            .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);

        let child = cmd.spawn().context("failed to spawn codex-exec")?;
        return Ok(child);
    }
}
