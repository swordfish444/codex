//! Build-time information helpers (git commit hash, version, …).

/// Return the git commit hash that was recorded at compile time via the
/// `build.rs` build-script.  Falls back to the static string "unknown" when the
/// build script failed to determine the hash (e.g. when building from a
/// source tarball without the `.git` directory).
pub fn git_sha() -> &'static str {
    env!("GIT_SHA")
}
