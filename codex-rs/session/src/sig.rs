//! Small safe wrappers around a handful of `nix::sys::signal` calls that are
//! considered `unsafe` by the `nix` crate. By concentrating the `unsafe` blocks
//! in a single, well-audited module we can keep the rest of the codebase — and
//! in particular `spawn.rs` — entirely `unsafe`-free.

#[cfg(unix)]
use nix::sys::signal::{signal as nix_signal, SigHandler, Signal};

/// Safely ignore `SIGHUP` for the current process.
///
/// Internally this delegates to `nix::sys::signal::signal(…, SigIgn)` which is
/// marked *unsafe* because changing signal handlers can break invariants in
/// foreign code. In our very controlled environment we *only* ever install the
/// predefined, always-safe `SIG_IGN` handler, which is guaranteed not to cause
/// undefined behaviour. Therefore it is sound to wrap the call in `unsafe` and
/// expose it as a safe function.
#[cfg(unix)]
pub fn ignore_sighup() -> nix::Result<()> {
    // SAFETY: Installing the built-in `SIG_IGN` handler is always safe.
    unsafe { nix_signal(Signal::SIGHUP, SigHandler::SigIgn) }.map(|_| ())
}

#[cfg(not(unix))]
#[allow(clippy::unused_io_amount)]
pub fn ignore_sighup() -> std::io::Result<()> {
    // No-op on non-Unix platforms.
    Ok(())
}

