use std::io;
use std::net::TcpListener;
use std::path::Path;
use std::process::Child;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::Mutex;

use crate::server;

/// Represents a running login subprocess. The child can be killed by holding
/// the mutex and calling `kill()`.
#[derive(Debug, Clone)]
pub struct SpawnedLogin {
    pub child: Arc<Mutex<Child>>,
    pub stdout: Arc<Mutex<Vec<u8>>>,
    pub stderr: Arc<Mutex<Vec<u8>>>,
}

/// Spawn the Rust login server via the current executable ("codex login") and return a handle to its process.
pub fn spawn_login_with_chatgpt(codex_home: &Path) -> std::io::Result<SpawnedLogin> {
    let current_exe = std::env::current_exe()?;
    let mut cmd = std::process::Command::new(current_exe);
    cmd.arg("login")
        .env("CODEX_HOME", codex_home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn()?;

    let stdout_buf = Arc::new(Mutex::new(Vec::new()));
    let stderr_buf = Arc::new(Mutex::new(Vec::new()));

    if let Some(mut out) = child.stdout.take() {
        let buf = stdout_buf.clone();
        std::thread::spawn(move || {
            let mut tmp = Vec::new();
            let _ = std::io::copy(&mut out, &mut tmp);
            if let Ok(mut b) = buf.lock() {
                b.extend_from_slice(&tmp);
            }
        });
    }
    if let Some(mut err) = child.stderr.take() {
        let buf = stderr_buf.clone();
        std::thread::spawn(move || {
            let mut tmp = Vec::new();
            let _ = std::io::copy(&mut err, &mut tmp);
            if let Ok(mut b) = buf.lock() {
                b.extend_from_slice(&tmp);
            }
        });
    }

    Ok(SpawnedLogin {
        child: Arc::new(Mutex::new(child)),
        stdout: stdout_buf,
        stderr: stderr_buf,
    })
}

/// Entrypoint used by the CLI to run the local login server.
pub async fn login_with_chatgpt(
    codex_home: &Path,
    open_browser: bool,
    verbose: bool,
) -> std::io::Result<()> {
    let client_id = std::env::var("CODEX_CLIENT_ID")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| crate::CLIENT_ID.to_string());

    match TcpListener::bind(("127.0.0.1", server::DEFAULT_PORT)) {
        Ok(_sock) => {
            // release immediately; server will bind next
        }
        Err(e) => {
            if e.kind() == io::ErrorKind::AddrInUse {
                return Err(io::Error::new(io::ErrorKind::AddrInUse, e));
            }
        }
    }

    let codex_home = codex_home.to_path_buf();
    let client_id_cloned = client_id.clone();
    tokio::task::spawn_blocking(move || {
        let opts = server::LoginServerOptions {
            codex_home: codex_home.clone(),
            client_id: client_id_cloned,
            issuer: server::DEFAULT_ISSUER.to_string(),
            port: server::DEFAULT_PORT,
            open_browser,
            redeem_credits: true,
            expose_state_endpoint: false,
            testing_timeout_secs: None,
            verbose,
        };
        server::run_local_login_server_with_options(opts)
    })
    .await
    .map_err(|e| std::io::Error::other(format!("task join error: {e}")))??;
    Ok(())
}


