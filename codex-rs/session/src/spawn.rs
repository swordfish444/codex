//! Spawn detached Codex agent processes (exec, repl, tui).
//!
//! The *exec* and *repl* helpers reuse the original FIFO/pipe strategy while
//! the new **tui** flavour allocates a pseudo-terminal so the crossterm /
//! ratatui application sees a *real* tty.  A small socket fan-out forwards raw
//! bytes between the PTY and every `codex-session attach` client.

use crate::store::Paths;
use anyhow::{Context, Result};
use std::fs::OpenOptions;
use tokio::process::{Child, Command};

#[cfg(unix)]
use std::os::unix::process::CommandExt; // for pre_exec

// -----------------------------------------------------------------------------
// exec – non-interactive batch agent (stdin = /dev/null)

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

// -----------------------------------------------------------------------------
// repl – interactive but **line-oriented** (FIFO for stdin)

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
                if err.kind() != io::ErrorKind::AlreadyExists {
                    return Err(err).context("mkfifo failed");
                }
            }
        }

        // Open the FIFO read-write so `open()` does **not** block even though
        // no external writer is connected yet.
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
        anyhow::bail!("codex-repl sessions are not yet supported on Windows");
    }
}

// -----------------------------------------------------------------------------
// tui – full terminal UI (PTY + socket fan-out)

use bytes::Bytes;

#[cfg(unix)]
use {
    nix::unistd::dup,
    std::os::unix::io::{FromRawFd, IntoRawFd, RawFd},
};

/// Spawn `codex-tui` inside a pseudo-terminal and start the background
/// multiplexer so future `attach` commands can talk to it.
pub async fn spawn_tui(paths: &Paths, tui_args: &[String]) -> Result<Child> {
    #[cfg(unix)]
    {
        use std::io;

        // 1. PTY allocation ---------------------------------------------------
        let pty = nix::pty::openpty(None, None).context("openpty failed")?;

        let slave_fd: RawFd = pty.slave.into_raw_fd();
        let master_fd: RawFd = pty.master.into_raw_fd();

        // Ensure master_fd is inheritable (clear FD_CLOEXEC)
        {
            use nix::fcntl::{fcntl, FcntlArg, FdFlag};
            let _ = fcntl(master_fd, FcntlArg::F_SETFD(FdFlag::empty()));
        }

        // 2. Spawn codex-tui --------------------------------------------------
        let make_stdio = |fd: RawFd| unsafe { std::process::Stdio::from_raw_fd(fd) };
        let stdin = make_stdio(dup(slave_fd)?);
        let stdout = make_stdio(dup(slave_fd)?);
        let stderr = make_stdio(slave_fd);

        let mut tui_cmd = Command::new("codex-tui");
        tui_cmd.args(tui_args)
            .stdin(stdin)
            .stdout(stdout)
            .stderr(stderr);

        unsafe {
            tui_cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(io::Error::last_os_error());
                }
                libc::signal(libc::SIGHUP, libc::SIG_IGN);
                Ok(())
            });
        }

        let child = tui_cmd.spawn().context("failed to spawn codex-tui")?;

        // 3. Spawn mux helper process ---------------------------------------

        let sock_path = paths.dir.join("sock");
        if sock_path.exists() {
            let _ = std::fs::remove_file(&sock_path);
        }

        let current_exe = std::env::current_exe()?;
        let mut mux_cmd = std::process::Command::new(current_exe);
        mux_cmd.arg("__mux")
            .arg("--fd").arg(format!("{master_fd}"))
            .arg("--sock").arg(&sock_path)
            .arg("--log").arg(&paths.stdout)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        // Detach mux process (own session)
        unsafe {
            mux_cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }

        let _ = mux_cmd.spawn().context("failed to spawn mux helper")?;

        Ok(child)
    }

    #[cfg(not(unix))]
    {
        anyhow::bail!("tui sessions are only supported on Unix right now");
    }
}

#[cfg(unix)]
async fn spawn_client(
    sock: tokio::net::UnixStream,
    pty_write_fd: RawFd,
    tx: &tokio::sync::broadcast::Sender<Bytes>,
) {
    use tokio::io::AsyncWriteExt;

    let (mut s_read, mut s_write) = sock.into_split();

    // Clone PTY master *write* side for this client
    let pty_write = unsafe {
        tokio::fs::File::from_std(std::fs::File::from_raw_fd(pty_write_fd))
    };
    let mut pty_write = pty_write;

    // subscribe
    let mut rx = tx.subscribe();

    // socket → pty
    let to_pty = tokio::spawn(async move {
        tokio::io::copy(&mut s_read, &mut pty_write).await.ok();
    });

    // pty broadcast → socket
    let from_pty = tokio::spawn(async move {
        while let Ok(bytes) = rx.recv().await {
            if s_write.write_all(&bytes).await.is_err() {
                break;
            }
        }
    });

    let _ = tokio::join!(to_pty, from_pty);
}

/// Actual multiplexer event loop that runs inside the forked daemon process.
#[cfg(unix)]
#[cfg(unix)]
pub async fn mux_main(master_fd: RawFd, sock_path: std::path::PathBuf, stdout_log: std::path::PathBuf) -> anyhow::Result<()> {
    use tokio::{net::UnixListener, sync::broadcast};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // Bind socket (should succeed; stale already removed).
    let listener = match UnixListener::bind(&sock_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("socket bind failed: {e}");
            return Ok(());
        }
    };

    // Async read handle for PTY master.
    let master_read = unsafe {
        tokio::fs::File::from_std(std::fs::File::from_raw_fd(dup(master_fd).expect("dup master")))
    };

    // binary log file
    let mut log_file = match tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&stdout_log)
        .await
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("log open failed: {e}");
            return Ok(());
        }
    };

    let (tx, _) = broadcast::channel::<Bytes>(64);

    // Reader task
    let tx_read = tx.clone();
    tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        let mut r = master_read;
        loop {
            match r.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let _ = log_file.write_all(&buf[..n]).await;
                    let _ = tx_read.send(Bytes::copy_from_slice(&buf[..n]));
                }
                Err(e) => {
                    eprintln!("pty read error: {e}");
                    break;
                }
            }
        }
    });

    // Accept-loop
    loop {
        match listener.accept().await {
            Ok((sock, _)) => {
                match dup(master_fd) {
                    Ok(fd) => {
                        let tx_c = tx.clone();
                        tokio::spawn(async move {
                            spawn_client(sock, fd, &tx_c).await;
                        });
                    }
                    Err(e) => eprintln!("dup failed: {e}"),
                }
            }
            Err(e) => {
                eprintln!("accept error: {e}");
                break;
            }
        }
    }

    Ok(())
}

#[cfg(not(unix))]
pub async fn mux_main(_fd: i32, _sock: std::path::PathBuf, _log: std::path::PathBuf) -> anyhow::Result<()> {
    anyhow::bail!("tui sessions are only supported on unix");
}
