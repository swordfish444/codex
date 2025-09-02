**DOs**
- **Centralize binary discovery in Rust (arg0):** Move PATH augmentation and helper discovery into `codex-rs/arg0` so it works for npm, Homebrew, and GitHub Releases.
```rust
// codex-rs/arg0/src/lib.rs (example)
use std::{env, path::PathBuf};

pub fn augment_path_for_helpers() {
    let mut paths: Vec<PathBuf> = env::var_os("PATH")
        .map(env::split_paths)
        .map(|it| it.collect())
        .unwrap_or_default();

    // Add packaged bin alongside the installed binary (e.g., GH Releases, Homebrew).
    if let Ok(exe) = env::current_exe() {
        if let Some(bin) = exe.parent().and_then(|p| p.parent()).map(|p| p.join("bin")) {
            paths.insert(0, bin);
        }
    }

    // Add CODEX_HOME/<version> for helper shims (e.g., apply_patch).
    if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
        let ver = env!("CARGO_PKG_VERSION");
        paths.insert(0, home.join(".codex").join(ver));
    }

    if let Ok(joined) = env::join_paths(paths) {
        env::set_var("PATH", joined);
    }
}
```

- **Support stdin when no args (apply-patch):** If zero args are given, read the patch payload from stdin; if one arg is given, use it; more than one is a usage error.
```rust
// codex-rs/apply-patch/src/main.rs (example)
use std::{io::{Read, Write}, process::ExitCode};

fn main() -> ExitCode {
    let mut args = std::env::args_os();
    let _argv0 = args.next();

    let mut patch = String::new();
    match args.next() {
        Some(arg) => {
            if args.next().is_some() {
                eprintln!("Error: apply-patch accepts at most one argument.");
                return ExitCode::from(2);
            }
            match arg.into_string() {
                Ok(s) => patch = s,
                Err(_) => {
                    eprintln!("Error: PATCH must be valid UTF-8.");
                    return ExitCode::from(2);
                }
            }
        }
        None => {
            if let Err(e) = std::io::stdin().read_to_string(&mut patch) {
                eprintln!("Error reading stdin: {e}");
                return ExitCode::from(1);
            }
        }
    }

    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    match codex_apply_patch::apply_patch(&patch, &mut stdout, &mut stderr) {
        Ok(()) => {
            let _ = stdout.flush();
            ExitCode::from(0)
        }
        Err(_) => ExitCode::from(1),
    }
}
```

- **Add CLI tests for stdin behavior:** Validate both argument and stdin modes using `assert_cmd`.
```rust
// codex-rs/apply-patch/tests/cli.rs (example)
use assert_cmd::prelude::*;
use std::fs;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn stdin_mode_adds_file() -> anyhow::Result<()> {
    let tmp = tempdir()?;
    let file = "stdin_add.txt";
    let patch = format!(
        "*** Begin Patch\n*** Add File: {file}\n+hello\n*** End Patch\n"
    );

    Command::cargo_bin("apply-patch")?
        .current_dir(tmp.path())
        .write_stdin(patch)
        .assert()
        .success()
        .stdout(format!("Success. Updated the following files:\nA {file}\n"));

    assert_eq!(fs::read_to_string(tmp.path().join(file))?, "hello\n");
    Ok(())
}
```

**DON’Ts**
- **Don’t hardcode npm-only paths in the Node wrapper:** Avoid assuming `__dirname/../bin/...` exists; this breaks Homebrew and GH Releases installs. Centralize logic in `arg0` instead.
```js
// Anti-pattern: brittle outside npm installs
const binaryPath = path.join(__dirname, "..", "bin", `codex-${targetTriple}`);
```

- **Don’t require exactly one argument for apply-patch:** Zero args must be supported via stdin; reject more than one arg with a clear usage error.
```rust
// Anti-pattern: wrongly forces one arg and panics on none
let mut args = std::env::args();
let _ = args.next();
let patch = args.next().expect("PATCH required"); // incorrect
```