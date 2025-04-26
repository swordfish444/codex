// build.rs – emit the current git commit so the code can embed it in the
// session metadata file.

fn main() {
    // Try to run `git rev-parse HEAD` – if that fails we fall back to
    // "unknown" so the build does not break when the source is not a git
    // repository (e.g., during `cargo publish`).
    let git_sha = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .unwrap_or_else(|| "unknown".into());

    println!("cargo:rustc-env=GIT_SHA={git_sha}");
}
