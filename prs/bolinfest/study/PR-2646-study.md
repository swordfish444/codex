**DOs**

- Bold: Route by arg0: dispatch to the right entrypoint based on `argv[0]`.
```rust
let exe_name = std::env::current_exe()?
    .file_name().and_then(|s| s.to_str()).unwrap_or("");

if exe_name == LINUX_SANDBOX_ARG0 {
    codex_linux_sandbox::run_main(); // never returns
} else if exe_name == APPLY_PATCH_ARG0 || exe_name == MISSPELLED_APPLY_PATCH_ARG0 {
    codex_apply_patch::main(); // never returns
}
```

- Bold: Expose an `apply_patch` binary: add a bin target.
```toml
[[bin]]
name = "apply_patch"
path = "src/main.rs"
```

- Bold: Provide a standalone `main` for the binary.
```rust
// apply-patch/src/main.rs
pub fn main() -> ! {
    codex_apply_patch::main()
}
```

- Bold: Accept patch via single arg or stdin; reject extras; flush on success.
```rust
pub fn run_main() -> i32 {
    let mut args = std::env::args_os();
    let _argv0 = args.next();

    let patch = match args.next() {
        Some(arg) => match arg.into_string() {
            Ok(s) => s,
            Err(_) => { eprintln!("Error: apply_patch requires a UTF-8 PATCH argument."); return 1; }
        },
        None => {
            let mut buf = String::new();
            if std::io::stdin().read_to_string(&mut buf).is_err() || buf.is_empty() {
                eprintln!("Usage: apply_patch 'PATCH'\n       echo 'PATCH' | apply-patch");
                return 2;
            }
            buf
        }
    };
    if args.next().is_some() {
        eprintln!("Error: apply_patch accepts exactly one argument.");
        return 2;
    }

    let (mut out, mut err) = (std::io::stdout(), std::io::stderr());
    match crate::apply_patch(&patch, &mut out, &mut err) {
        Ok(()) => { let _ = out.flush(); 0 }
        Err(_) => 1,
    }
}
```

- Bold: Prepend PATH with a temp dir that provides `apply_patch` (and `applypatch`) before spawning threads.
```rust
fn prepend_path_entry_for_apply_patch() -> std::io::Result<TempDir> {
    let temp_dir = TempDir::new()?;
    let exe = std::env::current_exe()?;

    #[cfg(unix)]
    for name in [APPLY_PATCH_ARG0, MISSPELLED_APPLY_PATCH_ARG0] {
        std::os::unix::fs::symlink(&exe, temp_dir.path().join(name))?;
    }

    #[cfg(windows)]
    for name in [APPLY_PATCH_ARG0, MISSPELLED_APPLY_PATCH_ARG0] {
        let bat = temp_dir.path().join(format!("{name}.bat"));
        std::fs::write(&bat, format!("@echo off\r\n\"{}\" {} %*\r\n", exe.display(), CODEX_APPLY_PATCH_ARG1))?;
    }

    let sep = if cfg!(windows) { ";" } else { ":" };
    let updated = match std::env::var("PATH") {
        Ok(p) => format!("{}{}{}", temp_dir.path().display(), sep, p),
        Err(_) => format!("{}", temp_dir.path().display()),
    };
    std::env::set_var("PATH", updated);
    Ok(temp_dir)
}
```

- Bold: Retain the temp dir for process lifetime; warn if PATH injection fails.
```rust
load_dotenv();
// keep it alive; do this before starting Tokio/threads
let _path_entry = match prepend_path_entry_for_apply_patch() {
    Ok(td) => Some(td),
    Err(err) => { eprintln!("WARNING: proceeding, even though we could not update PATH: {err}"); None }
};
let runtime = tokio::runtime::Runtime::new()?; // safe now
```

- Bold: Support the alias `applypatch` everywhere you handle `apply_patch`.
```rust
for name in [APPLY_PATCH_ARG0, MISSPELLED_APPLY_PATCH_ARG0] {
    // create symlink or .bat for both
}
```

- Bold: Use inline `format!` placeholders to build patches and expected outputs.
```rust
let file = "cli_test.txt";
let add_patch = format!(
    r#"*** Begin Patch
*** Add File: {file}
+hello
*** End Patch"#
);
```

- Bold: Write integration tests with `assert_cmd`, covering both arg and stdin flows.
```rust
use assert_cmd::prelude::*;
use std::process::Command;

Command::cargo_bin("apply_patch")?
    .arg(add_patch)
    .current_dir(tmp.path())
    .assert()
    .success()
    .stdout(format!("Success. Updated the following files:\nA {file}\n"));

let mut cmd = assert_cmd::Command::cargo_bin("apply_patch")?;
cmd.current_dir(tmp.path());
cmd.write_stdin(update_patch)
    .assert()
    .success()
    .stdout(format!("Success. Updated the following files:\nM {file}\n"));
```

- Bold: Assume paths never contain NUL on UNIX/Windows; no extra checks required.
```rust
// Path NUL validation unnecessary; standard APIs suffice
let exe = std::env::current_exe()?; // safe for our purposes
```

**DON’Ts**

- Bold: Don’t exit if PATH injection fails; warn and continue.
```rust
let _path_entry = match prepend_path_entry_for_apply_patch() {
    Ok(td) => Some(td),
    Err(err) => { eprintln!("WARNING: proceeding, even though we could not update PATH: {err}"); None }
};
```

- Bold: Don’t spawn threads/runtime before modifying `PATH`.
```rust
// Correct order:
load_dotenv();
let _path_entry = prepend_path_entry_for_apply_patch().ok();
let runtime = tokio::runtime::Runtime::new()?; // after PATH change
```

- Bold: Don’t accept multiple CLI args; exactly one or read stdin.
```rust
if args.next().is_some() {
    eprintln!("Error: apply_patch accepts exactly one argument.");
    return 2;
}
```

- Bold: Don’t drop the temp dir handle; keep it alive.
```rust
// Keep `TempDir` in scope (e.g., Option<TempDir>) until process exit
let _path_entry: Option<TempDir> = prepend_path_entry_for_apply_patch().ok();
```

- Bold: Don’t mix platform logic; gate with `#[cfg(unix)]` / `#[cfg(windows)]`.
```rust
#[cfg(unix)]
std::os::unix::fs::symlink(&exe, temp_dir.path().join(APPLY_PATCH_ARG0))?;

#[cfg(windows)]
std::fs::write(temp_dir.path().join("apply_patch.bat"),
               format!("@echo off\r\n\"{}\" {} %*\r\n", exe.display(), CODEX_APPLY_PATCH_ARG1))?;
```

- Bold: Don’t rely on a separately installed `apply_patch`; inject it via PATH.
```rust
// Create/link `apply_patch` into a TempDir and prepend to PATH
let _path_entry = prepend_path_entry_for_apply_patch()?;
```

- Bold: Don’t forget the `applypatch` alias.
```rust
if exe_name == APPLY_PATCH_ARG0 || exe_name == MISSPELLED_APPLY_PATCH_ARG0 {
    codex_apply_patch::main();
}
```

- Bold: Don’t build strings with manual concatenation; use `format!` with placeholders.
```rust
let msg = format!("Success. Updated the following files:\nA {file}\n");
```