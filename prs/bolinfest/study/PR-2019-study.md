**DOs**

- **Use OS temp files for long scripts:** Write embedded scripts to a temp file to avoid Windows error 206 (long command lines).
```rust
use std::io::Write;
use tempfile::NamedTempFile;

fn ensure_login_script() -> std::io::Result<NamedTempFile> {
    let mut file = NamedTempFile::new()?;
    file.write_all(SOURCE_FOR_PYTHON_SERVER.as_bytes())?;
    file.flush()?;
    Ok(file)
}
```

- **Keep the temp file alive:** Store the `NamedTempFile` so it isn’t deleted while a child process still needs it.
```rust
use std::process::{Child, Command};
use tempfile::NamedTempFile;

pub struct SpawnedLogin {
    child: Child,
    _script: NamedTempFile, // holds file alive until drop
}

pub fn spawn_login_with_chatgpt() -> std::io::Result<SpawnedLogin> {
    let script_file = ensure_login_script()?;
    let script_path = script_file.path();

    let child = Command::new("python3")
        .arg(script_path)
        .spawn()?;

    Ok(SpawnedLogin { child, _script: script_file })
}
```

- **Prefer clear names:** Use `script_file` (the RAII handle) and `script_path` (a `&Path`) to make lifetimes and intent obvious.
```rust
let script_file = ensure_login_script()?;
let script_path = script_file.path();
```

- **Fallback to `keep()` when threading lifetimes is invasive:** If passing `NamedTempFile` through many layers is too costly, persist the file and return a `PathBuf`.
```rust
use std::io::Write;
use std::path::PathBuf;
use tempfile::NamedTempFile;

fn write_login_script_to_disk() -> std::io::Result<PathBuf> {
    let mut tmp = NamedTempFile::new()?;
    tmp.write_all(SOURCE_FOR_PYTHON_SERVER.as_bytes())?;
    tmp.flush()?;
    let (_f, path) = tmp.keep()?; // file persists beyond drop
    Ok(path)
}
```

- **Gracefully handle unsupported terminal features:** Attempt to push/pop keyboard enhancement flags, but ignore errors on platforms that don’t support them.
```rust
use crossterm::{execute, terminal::disable_raw_mode};
use crossterm::event::{PushKeyboardEnhancementFlags, PopKeyboardEnhancementFlags, KeyboardEnhancementFlags};
use std::io::stdout;

// init
let _ = execute!(
    stdout(),
    PushKeyboardEnhancementFlags(
        KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
            | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
            | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
    )
);

// restore
let _ = execute!(stdout(), PopKeyboardEnhancementFlags);
disable_raw_mode()?;
```

- **Pass script path to Python, not inline code:** Invoke `python3` with the script path to avoid argument-length limits.
```rust
Command::new("python3").arg(script_path).spawn()?;
```

**DON’Ts**

- **Don’t inline huge scripts with `-c`:** This risks Windows error 206 and hurts portability.
```rust
// ❌ Anti-pattern: giant inline string
Command::new("python3")
    .arg("-c")
    .arg(SOURCE_FOR_PYTHON_SERVER) // long arg, fragile on Windows
    .spawn()?;
```

- **Don’t write temp artifacts under `CODEX_HOME`:** Prefer OS temp directories so files are ephemeral and not user-visible.
```rust
// ❌ Avoid: writing temp script into user config dir
let path = codex_home.join("login_server.py"); // not a temp location
```

- **Don’t drop `NamedTempFile` before the child is done:** Returning a `PathBuf` from a `NamedTempFile` without `keep()` unlinks the file on drop.
```rust
// ❌ Broken: file disappears when `tmp` drops
fn path_only() -> std::io::Result<std::path::PathBuf> {
    let tmp = NamedTempFile::new()?;
    Ok(tmp.path().to_path_buf()) // deleted when `tmp` drops
}
```

- **Don’t crash TUI on unsupported features:** Using `?` on `execute!(..., Push/PopKeyboardEnhancementFlags)` can abort on legacy consoles.
```rust
// ❌ Fragile: fails hard on unsupported terminals
execute!(stdout(), PopKeyboardEnhancementFlags)?; // prefer `let _ = ...`
```