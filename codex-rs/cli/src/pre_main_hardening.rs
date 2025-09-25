#[cfg(target_os = "linux")]
const PRCTL_FAILED_EXIT_CODE: i32 = 5;

#[cfg(target_os = "macos")]
const PTRACE_DENY_ATTACH_FAILED_EXIT_CODE: i32 = 6;

#[cfg(any(target_os = "linux", target_os = "macos"))]
const SET_RLIMIT_CORE_FAILED_EXIT_CODE: i32 = 7;

#[cfg(windows)]
const DEBUG_SET_PROCESS_KILL_ON_EXIT_FAILED_EXIT_CODE: i32 = 8;

#[cfg(windows)]
const WER_SET_FLAGS_FAILED_EXIT_CODE: i32 = 9;

#[cfg(windows)]
const WINDOWS_PROFILER_ENV_VARS: [&str; 11] = [
    "COR_ENABLE_PROFILING",
    "COR_PROFILER",
    "COR_PROFILER_PATH",
    "COR_PROFILER_PATH_32",
    "COR_PROFILER_PATH_64",
    "CORECLR_ENABLE_PROFILING",
    "CORECLR_PROFILER",
    "CORECLR_PROFILER_PATH",
    "CORECLR_PROFILER_PATH_32",
    "CORECLR_PROFILER_PATH_64",
    "DOTNET_STARTUP_HOOKS",
];

#[cfg(target_os = "linux")]
pub(crate) fn pre_main_hardening_linux() {
    // Disable ptrace attach / mark process non-dumpable.
    unsafe {
        if libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) != 0 {
            eprintln!(
                "ERROR: prctl(PR_SET_DUMPABLE, 0) failed: {}",
                std::io::Error::last_os_error()
            );
            std::process::exit(PRCTL_FAILED_EXIT_CODE);
        }
    }

    // For "defense in depth," set the core file size limit to 0.
    unsafe {
        let rlim = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if libc::setrlimit(libc::RLIMIT_CORE, &rlim) != 0 {
            eprintln!(
                "ERROR: setrlimit(RLIMIT_CORE) failed: {}",
                std::io::Error::last_os_error()
            );
            std::process::exit(SET_RLIMIT_CORE_FAILED_EXIT_CODE);
        }
    }

    // Official Codex releases are MUSL-linked, which means that variables such
    // as LD_PRELOAD are ignored anyway, but just to be sure, clear them here.
    let ld_keys: Vec<String> = std::env::vars()
        .filter_map(|(key, _)| {
            if key.starts_with("LD_") {
                Some(key)
            } else {
                None
            }
        })
        .collect();

    for key in ld_keys {
        unsafe {
            std::env::remove_var(key);
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn pre_main_hardening_macos() {
    unsafe {
        if libc::ptrace(libc::PT_DENY_ATTACH, 0, std::ptr::null_mut(), 0) == -1 {
            eprintln!(
                "ERROR: ptrace(PT_DENY_ATTACH) failed: {}",
                std::io::Error::last_os_error()
            );
            std::process::exit(PTRACE_DENY_ATTACH_FAILED_EXIT_CODE);
        }
    }

    unsafe {
        let rlim = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if libc::setrlimit(libc::RLIMIT_CORE, &rlim) != 0 {
            eprintln!(
                "ERROR: setrlimit(RLIMIT_CORE) failed: {}",
                std::io::Error::last_os_error()
            );
            std::process::exit(SET_RLIMIT_CORE_FAILED_EXIT_CODE);
        }
    }

    // Remove all DYLD_ environment variables, which can be used to
    // subvert library loading.
    let dyld_keys: Vec<String> = std::env::vars()
        .filter_map(|(key, _)| {
            if key.starts_with("DYLD_") {
                Some(key)
            } else {
                None
            }
        })
        .collect();

    for key in dyld_keys {
        unsafe {
            std::env::remove_var(key);
        }
    }
}

#[cfg(windows)]
pub(crate) fn pre_main_hardening_windows() {
    unsafe {
        use windows_sys::Win32::Foundation::TRUE;
        use windows_sys::Win32::System::Diagnostics::Debug::DebugSetProcessKillOnExit;
        use windows_sys::Win32::System::Diagnostics::Debug::SEM_FAILCRITICALERRORS;
        use windows_sys::Win32::System::Diagnostics::Debug::SEM_NOGPFAULTERRORBOX;
        use windows_sys::Win32::System::Diagnostics::Debug::SEM_NOOPENFILEERRORBOX;
        use windows_sys::Win32::System::Diagnostics::Debug::SetErrorMode;
        use windows_sys::Win32::System::ErrorReporting::WER_FAULT_REPORTING_FLAG_NOHEAP;
        use windows_sys::Win32::System::ErrorReporting::WER_FAULT_REPORTING_FLAG_QUEUE;
        use windows_sys::Win32::System::ErrorReporting::WER_FAULT_REPORTING_NO_UI;
        use windows_sys::Win32::System::ErrorReporting::WerSetFlags;

        SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX | SEM_NOOPENFILEERRORBOX);

        if DebugSetProcessKillOnExit(TRUE) == 0 {
            eprintln!(
                "ERROR: DebugSetProcessKillOnExit(TRUE) failed: {}",
                std::io::Error::last_os_error()
            );
            std::process::exit(DEBUG_SET_PROCESS_KILL_ON_EXIT_FAILED_EXIT_CODE);
        }

        let flags = WER_FAULT_REPORTING_NO_UI
            | WER_FAULT_REPORTING_FLAG_NOHEAP
            | WER_FAULT_REPORTING_FLAG_QUEUE;
        let hr = WerSetFlags(flags);
        if hr < 0 {
            eprintln!("ERROR: WerSetFlags() failed with HRESULT {hr:#010x}");
            std::process::exit(WER_SET_FLAGS_FAILED_EXIT_CODE);
        }
    }

    for var in WINDOWS_PROFILER_ENV_VARS {
        unsafe {
            std::env::remove_var(var);
        }
    }
}
