use clap::Parser;
use codex_common::SandboxPermissionOption;
use std::ffi::CString;

use crate::landlock::apply_sandbox_policy_to_current_thread;

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

    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            panic!("failed to getcwd(): {e:?}");
        }
    };

    if let Err(e) = apply_sandbox_policy_to_current_thread(&sandbox_policy, &cwd) {
        panic!("error running landlock: {e:?}");
    }

    if command.is_empty() {
        panic!("No command specified to execute.");
    }

    let c_command =
        CString::new(command[0].as_str()).expect("Failed to convert command to CString");
    let c_args: Vec<CString> = command
        .iter()
        .map(|arg| CString::new(arg.as_str()).expect("Failed to convert arg to CString"))
        .collect();

    let mut c_args_ptrs: Vec<*const libc::c_char> = c_args.iter().map(|arg| arg.as_ptr()).collect();
    c_args_ptrs.push(std::ptr::null());

    unsafe {
        libc::execv(c_command.as_ptr(), c_args_ptrs.as_ptr());
    }

    // If execv returns, there was an error.
    let err = std::io::Error::last_os_error();
    panic!("Failed to execv: {err}");
}
