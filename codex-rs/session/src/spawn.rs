//! Spawn detached Codex agent processes for exec and repl sessions.

use crate::store::Paths;
use anyhow::Context;
use anyhow::Result;
use std::fs::OpenOptions;
use tokio::process::Child;
use tokio::process::Command;

#[cfg(unix)]
use command_group::AsyncCommandGroup;
#[cfg(unix)]
use nix::errno::Errno;
#[cfg(unix)]
use nix::sys::stat::Mode;
#[cfg(unix)]
use nix::unistd::mkfifo;

/// Open (and create if necessary) the log files that stdout / stderr of the
/// spawned agent will be redirected to.
fn open_log_files(paths: &Paths) -> Result<(std::fs::File, std::fs::File)> {
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&paths.stdout)?;

    let stderr = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&paths.stderr)?;

    Ok((stdout, stderr))
}

/// Configure a `tokio::process::Command` with the common options that are the
/// same for both `codex-exec` and `codex-repl` sessions.
fn base_command(bin: &str, paths: &Paths) -> Result<Command> {
    let (stdout, stderr) = open_log_files(paths)?;

    let mut cmd = Command::new(bin);
    cmd.stdin(std::process::Stdio::null())
        .stdout(stdout)
        .stderr(stderr);

    Ok(cmd)
}

pub fn spawn_exec(paths: &Paths, exec_args: &[String]) -> Result<Child> {
    #[cfg(unix)]
    {
        // Build the base command and add the user-supplied arguments.
        let mut cmd = base_command("codex-exec", paths)?;
        cmd.args(exec_args);

        // exec is non-interactive, use /dev/null for stdin.
        let stdin = OpenOptions::new().read(true).open("/dev/null")?;
        cmd.stdin(stdin);

        // Spawn the child as a process group / new session leader.
        let child = cmd
            .group_spawn()
            .context("failed to spawn codex-exec")?
            .into_inner();

        crate::sig::ignore_sighup()?;

        Ok(child)
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        const DETACHED_PROCESS: u32 = 0x00000008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;

        let mut cmd = base_command("codex-exec", paths)?;
        cmd.args(exec_args)
            .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);

        let child = cmd.spawn().context("failed to spawn codex-exec")?;
        Ok(child)
    }
}

pub fn spawn_repl(paths: &Paths, repl_args: &[String]) -> Result<Child> {
    #[cfg(unix)]
    {
        // Ensure a FIFO exists at `paths.stdin` with permissions rw-------
        if !paths.stdin.exists() {
            if let Err(e) = mkfifo(&paths.stdin, Mode::from_bits_truncate(0o600)) {
                // If the FIFO already exists we silently accept, just as the
                // previous implementation did.
                if e != Errno::EEXIST {
                    return Err(std::io::Error::from(e)).context("mkfifo failed");
                }
            }
        }

        // Open the FIFO for *both* reading and writing so we don't deadlock
        // when there is no writer yet (mimics the previous behaviour).
        let stdin = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&paths.stdin)?;

        // Build the command.
        let mut cmd = base_command("codex-repl", paths)?;
        cmd.args(repl_args).stdin(stdin);

        // Detached spawn.
        let child = cmd
            .group_spawn()
            .context("failed to spawn codex-repl")?
            .into_inner();

        crate::sig::ignore_sighup()?;

        Ok(child)
    }

    #[cfg(windows)]
    {
        anyhow::bail!("codex-repl sessions are not supported on Windows yet");
    }
}
