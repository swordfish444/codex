#[cfg(target_os = "linux")]
mod landlock;
#[cfg(target_os = "linux")]
mod linux_run_main;

#[cfg(target_os = "linux")]
pub use codex_linux_sandbox::run_main;

#[cfg(not(target_os = "linux"))]
pub fn run_main() -> ! {
    panic!("codex-linux-sandbox is only supported on Linux");
}
