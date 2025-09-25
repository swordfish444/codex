#[cfg(any(target_os = "linux", target_os = "android"))]
const PRCTL_FAILED_EXIT_CODE: i32 = 5;

#[cfg(target_os = "macos")]
const PTRACE_DENY_ATTACH_FAILED_EXIT_CODE: i32 = 6;

#[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
const SET_RLIMIT_CORE_FAILED_EXIT_CODE: i32 = 7;

#[cfg(any(target_os = "linux", target_os = "android"))]
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
    // TODO(mbolin): Perform the appropriate configuration for Windows.
}
