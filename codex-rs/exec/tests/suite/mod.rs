// Aggregates all former standalone integration tests as modules.
mod apply_patch;
mod common;
mod resume;
mod sandbox;
#[cfg(target_os = "windows")]
mod windows_sandbox;
