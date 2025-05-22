//! Helper for propagating the exit status of a sandboxed child process to the
//! parent process (i.e. `codex-exec` when used as a Linux sandbox wrapper).

#[cfg(unix)]
pub(crate) fn handle_exit_status(status: std::process::ExitStatus) -> ! {
    use std::os::unix::process::ExitStatusExt;

    if let Some(code) = status.code() {
        std::process::exit(code);
    } else if let Some(signal) = status.signal() {
        std::process::exit(128 + signal);
    } else {
        // Fallback – unknown termination reason.
        std::process::exit(1);
    }
}

#[cfg(windows)]
pub(crate) fn handle_exit_status(status: std::process::ExitStatus) -> ! {
    if let Some(code) = status.code() {
        std::process::exit(code);
    } else {
        std::process::exit(1);
    }
}
