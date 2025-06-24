use clap::Parser;
use codex_common::SandboxPermissionOption;
use std::ffi::CString;

use libc;

use crate::landlock::apply_sandbox_policy_to_current_thread;
use codex_core::config::{Config, ConfigOverrides};
use codex_core::util::{find_git_root, relative_path_from_git_root};

#[derive(Debug, Parser)]
pub struct LandlockCommand {
    #[clap(flatten)]
    pub sandbox: SandboxPermissionOption,

    /// Full command args to run under landlock.
    #[arg(trailing_var_arg = true)]
    pub command: Vec<String>,
}

pub fn run_main() -> ! {
    let LandlockCommand { sandbox, command } = LandlockCommand::parse();

    let sandbox_policy = match sandbox.permissions.map(Into::into) {
        Some(sandbox_policy) => sandbox_policy,
        None => codex_core::protocol::SandboxPolicy::new_read_only_policy(),
    };

    // Determine working directory inside the session, possibly auto-mounting the repo.
    let mut cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => panic!("failed to getcwd(): {e:?}"),
    };
    // Load configuration to check auto_mount_repo flag
    let config = match codex_core::config::Config::load_with_cli_overrides(
        Vec::new(),
        codex_core::config::ConfigOverrides::default(),
    ) {
        Ok(cfg) => cfg,
        Err(e) => panic!("failed to load config for auto-mount: {e:?}"),
    };
    if config.tui.auto_mount_repo {
        if let Some(root) = codex_core::util::find_git_root(&cwd) {
            // Compute relative subpath
            let rel = codex_core::util::relative_path_from_git_root(&cwd).unwrap_or_default();
            let mount_prefix = std::path::PathBuf::from(&config.tui.mount_prefix);
            // Create mount target
            std::fs::create_dir_all(&mount_prefix).unwrap_or_else(|e| {
                panic!("failed to create mount prefix {mount_prefix:?}: {e:?}")
            });
            // Bind-mount repository root into session
            let src = std::ffi::CString::new(root.to_string_lossy().as_ref())
                .expect("invalid git root path");
            let dst = std::ffi::CString::new(mount_prefix.to_string_lossy().as_ref())
                .expect("invalid mount prefix path");
            unsafe {
                libc::mount(
                    src.as_ptr(),
                    dst.as_ptr(),
                    std::ptr::null(),
                    libc::MS_BIND,
                    std::ptr::null(),
                );
            }
            // Change working directory to corresponding subfolder under mount
            cwd = mount_prefix.join(rel);
            std::env::set_current_dir(&cwd)
                .unwrap_or_else(|e| panic!("failed to chdir to {cwd:?}: {e:?}"));
        }
    }

    if let Err(e) = apply_sandbox_policy_to_current_thread(&sandbox_policy, &cwd) {
        panic!("error running landlock: {e:?}");
    }

    if command.is_empty() {
        panic!("No command specified to execute.");
    }

    #[expect(clippy::expect_used)]
    let c_command =
        CString::new(command[0].as_str()).expect("Failed to convert command to CString");
    #[expect(clippy::expect_used)]
    let c_args: Vec<CString> = command
        .iter()
        .map(|arg| CString::new(arg.as_str()).expect("Failed to convert arg to CString"))
        .collect();

    let mut c_args_ptrs: Vec<*const libc::c_char> = c_args.iter().map(|arg| arg.as_ptr()).collect();
    c_args_ptrs.push(std::ptr::null());

    unsafe {
        libc::execvp(c_command.as_ptr(), c_args_ptrs.as_ptr());
    }

    // If execvp returns, there was an error.
    let err = std::io::Error::last_os_error();
    panic!("Failed to execvp {}: {err}", command[0].as_str());
}
