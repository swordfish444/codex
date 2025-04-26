//! Spawn detached Codex agent processes.
//!
//! The session manager supports multiple agent flavors.  `codex-exec` requires no interactive
//! stdin so we can safely redirect it to `/dev/null`.  `codex-repl` however needs to read user
//! input after it is launched.  The background process therefore receives a **named pipe** as
//! its standard input which later `codex-session attach` commands can open for writing.

use crate::store::Paths;
use anyhow::Context;
use anyhow::Result;
use std::fs::OpenOptions;
use tokio::process::Child;
use tokio::process::Command;

/// Spawn a `codex-exec` agent.
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

        // Detach session so the child is not killed with the parent.
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

/// Spawn a `codex-repl` agent.  The process is detached like `spawn_exec` but its standard input
/// is connected to a named pipe inside the session directory so additional CLI instances can
/// attach later and feed user input.
pub fn spawn_repl(paths: &Paths, repl_args: &[String]) -> Result<Child> {
    #[cfg(unix)]
    {
        use std::io;
        use std::os::unix::ffi::OsStrExt;

        // Ensure the FIFO exists (create with 600 permissions).
        if !paths.stdin.exists() {
            let c_path = std::ffi::CString::new(paths.stdin.as_os_str().as_bytes()).unwrap();
            // SAFETY: libc call, check return value.
            let res = unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) };
            if res != 0 {
                let err = std::io::Error::last_os_error();
                // Ignore EEXIST if some race created it first.
                if err.kind() != io::ErrorKind::AlreadyExists {
                    return Err(err).context("mkfifo failed");
                }
            }
        }

        // Open the FIFO read-write so `open()` does **not** block even though no external writer
        // is connected yet.  Keeping the write end open inside the child prevents an EOF on the
        // read end while no `attach` session is active.
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
        return Ok(child);
    }

    #[cfg(windows)]
    {
        anyhow::bail!("codex-repl background sessions are not yet supported on Windows");
    }
}
