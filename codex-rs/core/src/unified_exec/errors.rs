use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum UnifiedExecError {
    #[error("Failed to create unified exec session: {pty_error}")]
    CreateSession {
        #[source]
        pty_error: anyhow::Error,
    },
    #[error("Unknown session id {session_id}")]
    UnknownSessionId { session_id: i32 },
    #[error("failed to write to stdin")]
    WriteToStdin,
    #[error("missing command line for unified exec request")]
    MissingCommandLine,
    #[error("missing codex-linux-sandbox executable path")]
    MissingLinuxSandboxExecutable,
    #[error("unified exec command rejected by user")]
    UserRejected,
}

impl UnifiedExecError {
    pub(crate) fn create_session(error: anyhow::Error) -> Self {
        Self::CreateSession { pty_error: error }
    }
}
