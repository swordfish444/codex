//! Spawn detached Codex agent processes for exec and repl sessions.

use crate::store::Paths;
use anyhow::{Context, Result};
use std::fs::OpenOptions;
use tokio::process::{Child, Command};

// -----------------------------------------------------------------------------
// exec – non-interactive batch agent

pub fn spawn_exec(paths: &Paths, exec_args: &[String]) -> Result<Child> {
    #[cfg(unix)]
    {
        use std::io;

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
        Ok(child)
    }
}

// -----------------------------------------------------------------------------
// repl – interactive FIFO stdin

pub fn spawn_repl(paths: &Paths, repl_args: &[String]) -> Result<Child> {
    #[cfg(unix)]
    {
        use std::io;
        use std::os::unix::ffi::OsStrExt;

        if !paths.stdin.exists() {
            let c_path = std::ffi::CString::new(paths.stdin.as_os_str().as_bytes()).unwrap();
            let res = unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) };
            if res != 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() != io::ErrorKind::AlreadyExists {
                    return Err(err).context("mkfifo failed");
                }
            }
        }

        let stdin = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&paths.stdin)?;

        let stdout = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&paths.stdout)?;
        let stderr = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&paths.stderr)?;

        let mut cmd = Command::new("codex-repl");
        cmd.args(repl_args)
            .stdin(stdin)
            .stdout(stdout)
            .stderr(stderr);

        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(io::Error::last_os_error());
                }
                libc::signal(libc::SIGHUP, libc::SIG_IGN);
                Ok(())
            });
        }

        let child = cmd.spawn().context("failed to spawn codex-repl")?;
        Ok(child)
    }

    #[cfg(windows)]
    {
        anyhow::bail!("codex-repl sessions are not supported on Windows yet");
    }
}

