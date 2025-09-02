**DOs**
- Rename functions purposefully: use names that match expanded behavior like translating commands, not just using a profile.
```rust
// Before
fn maybe_run_with_user_profile(params: ExecParams, sess: &Session, turn: &TurnContext) -> ExecParams { /* ... */ }

// After
fn maybe_translate_shell_command(params: ExecParams, sess: &Session, turn: &TurnContext) -> ExecParams { /* ... */ }

// Call site
let params = maybe_translate_shell_command(params, sess, turn_context);
```

- Pass rich types: prefer `Shell` over `String` in `EnvironmentContext` for type safety and flexibility.
```rust
pub(crate) struct EnvironmentContext {
    pub cwd: PathBuf,
    pub approval_policy: AskForApproval,
    pub sandbox_mode: SandboxMode,
    pub network_access: NetworkAccess,
    pub shell: Shell,
}

impl Display for EnvironmentContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Current working directory: {}", self.cwd.display())?;
        writeln!(f, "Approval policy: {}", self.approval_policy)?;
        writeln!(f, "Sandbox mode: {}", self.sandbox_mode)?;
        writeln!(f, "Network access: {}", self.network_access)?;
        if let Some(name) = self.shell.name() {
            writeln!(f, "Shell: {name}")?;
        }
        Ok(())
    }
}
```

- Use `PathBuf` for executables: model filesystem paths correctly.
```rust
#[derive(Clone)]
pub struct PowerShellConfig {
    pub exe: String,
    pub bash_exe_fallback: Option<PathBuf>,
}

// Detect Git Bash on Windows
let bash_exe = which::which("bash.exe").ok();
let ps = PowerShellConfig { exe: "pwsh.exe".into(), bash_exe_fallback: bash_exe };
```

- Prefer canonical `match`/`Option` patterns: keep flow clear and idiomatic.
```rust
// Prefer matching the fallback in one place
return match &ps.bash_exe_fallback {
    Some(bash) => Some(vec![bash.to_string_lossy().to_string(), "-lc".into(), script]),
    None => Some(vec![ps.exe.clone(), "-NoProfile".into(), "-Command".into(), script]),
};

// Prefer mapping Options
let joined = join_as_powershell_command(&command); // returns Option<String>
return joined.map(|arg| vec![ps.exe.clone(), "-NoProfile".into(), "-Command".into(), arg]);
```

- Detect Git Bash and prefer it when model generated `bash -lc ...`: run bash commands in bash when possible.
```rust
if let Some(script) = strip_bash_lc(&command) {
    return match &ps.bash_exe_fallback {
        Some(bash) => Some(vec![bash.to_string_lossy().to_string(), "-lc".into(), script]),
        None => Some(vec![ps.exe.clone(), "-NoProfile".into(), "-Command".into(), script]),
    };
}
```

- Use Windows-appropriate quoting for PowerShell: avoid POSIX quoting; escape for PS.
```rust
fn quote_ps(arg: &str) -> String {
    // PowerShell single-quoted string; escape single quotes by doubling.
    format!("'{}'", arg.replace('\'', "''"))
}

fn join_as_powershell_command(args: &[String]) -> Option<String> {
    // Heuristic: if the first token is a command, join the rest as arguments.
    // Real-world code should special-case known commands or build PS ASTs.
    Some(args.join(" ")) // Keep simple; avoid POSIX shlex on Windows.
}

// Build invocation
if command.first().map(String::as_str) != Some(ps.exe.as_str()) {
    let script = join_as_powershell_command(&command)?;
    return Some(vec![ps.exe.clone(), "-NoProfile".into(), "-Command".into(), script]);
}
```

- Handle multiline commands explicitly: support them deliberately, don’t rely on brittle heuristics.
```rust
// Example: pass a multi-line script to PowerShell safely
let script = r#"
$ErrorActionPreference = 'Stop'
Get-ChildItem -Force
"#.to_string();

let cmd = vec![ps.exe.clone(), "-NoProfile".into(), "-Command".into(), script];
```

- Parallelize shell detection at startup: probe executables concurrently to reduce latency.
```rust
use tokio::{join, process::Command};

async fn ok(cmd: &str, args: &[&str]) -> bool {
    Command::new(cmd).args(args).output().await.ok().map(|o| o.status.success()).unwrap_or(false)
}

let (has_pwsh, has_bash) = join!(
    ok("pwsh", &["-NoLogo", "-NoProfile", "-Command", "$PSVersionTable.PSVersion.Major"]),
    ok("bash.exe", &["--version"]),
);

let bash_exe = if has_bash { which::which("bash.exe").ok() } else { None };
```

- Gate behavior by platform clearly: use precise `cfg` combinations for non-macOS, non-Windows paths.
```rust
#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
pub async fn default_user_shell() -> Shell {
    Shell::Unknown
}
```

**DON’Ts**
- Assume bash scripts will run in PowerShell: prefer executing bash scripts with bash when available.
```rust
// Don’t blindly rewrite:
Some(vec!["pwsh.exe".into(), "-NoProfile".into(), "-Command".into(), script]) // may break many bash-isms
```

- Rely on POSIX `shlex` for Windows quoting: it doesn’t match PowerShell or Win32 rules.
```rust
// Don’t:
let joined = shlex::try_join(command.iter().map(|s| s.as_str())).ok(); // POSIX semantics on Windows
```

- Silently skip translation because of newlines: handle multi-line commands intentionally.
```rust
// Don’t:
if command.iter().any(|a| a.contains('\n') || a.contains('\r')) {
    return Some(command); // hides real issues; be explicit instead
}
```

- Store executable paths as `String`: use `PathBuf` for anything filesystem-like.
```rust
// Don’t:
struct PowerShellConfig { exe: String, bash_exe_fallback: Option<String> }
```

- Run shell probing sequentially on hot paths: it increases startup time.
```rust
// Don’t:
let has_pwsh = ok("pwsh", &[]).await;
let has_bash = ok("bash.exe", &[]).await; // do these with tokio::join!
```

- Complicate `Option` flow with manual `if/else` when `map`/`match` is clearer and safer.
```rust
// Don’t:
if let Some(joined) = joined { return Some(vec![/* ... */, joined]); }
return None;

// Prefer:
return joined.map(|arg| vec![/* ... */, arg]);
```