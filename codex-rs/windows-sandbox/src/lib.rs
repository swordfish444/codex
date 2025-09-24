#[cfg(target_os = "windows")]
mod windows_run_main;

#[cfg(target_os = "windows")]
pub const WINDOWS_SANDBOX_ARG1: &str = "--codex-run-as-windows-sandbox";

#[cfg(not(target_os = "windows"))]
pub const WINDOWS_SANDBOX_ARG1: &str = "--codex-run-as-windows-sandbox";

#[cfg(target_os = "windows")]
pub fn run_main() -> ! {
    windows_run_main::run_main();
}

#[cfg(not(target_os = "windows"))]
pub fn run_main() -> ! {
    panic!("codex-windows-sandbox is only supported on Windows");
}
