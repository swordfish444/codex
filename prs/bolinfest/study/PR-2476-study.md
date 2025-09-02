**PR 2476 Review Takeaways: Git Diff Command**

**DOs**
- Bold types: use strong types for SHAs.
```rust
use codex_protocol::mcp_protocol::GitSha;

#[derive(Clone, Debug)]
struct GitDiffToRemote {
    sha: GitSha,
    diff: String,
}

// Construct safely
let sha = GitSha::new(&remote_sha_str);
```

- Centralize git env isolation: set env vars in the git runner.
```rust
use tokio::process::Command;
use tokio::time::{timeout, Duration};

const GIT_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

async fn run_git_command_with_timeout(args: &[&str], cwd: &std::path::Path) -> Option<std::process::Output> {
    let mut cmd = Command::new("git");
    cmd.args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .current_dir(cwd);

    let out = timeout(GIT_COMMAND_TIMEOUT, cmd.output()).await.ok()?;
    out.ok()
}
```

- Use a helper for repo detection; return early for non-git dirs.
```rust
use codex_core::util::is_inside_git_repo;

pub async fn git_diff_to_remote(cwd: &std::path::Path) -> Option<GitDiffToRemote> {
    if !is_inside_git_repo(cwd) {
        return None;
    }

    // ...rest of logic
}
```

- Prioritize `origin` and detect default branch robustly.
```rust
async fn get_git_remotes(cwd: &std::path::Path) -> Option<Vec<String>> {
    let output = run_git_command_with_timeout(&["remote"], cwd).await?;
    if !output.status.success() {
        return None;
    }

    let mut remotes: Vec<String> = String::from_utf8(output.stdout).ok()?
        .lines().map(|s| s.to_string()).collect();

    if let Some(pos) = remotes.iter().position(|r| r == "origin") {
        let origin = remotes.remove(pos);
        remotes.insert(0, origin);
    }
    Some(remotes)
}

async fn get_default_branch(cwd: &std::path::Path) -> Option<String> {
    for remote in get_git_remotes(cwd).await.unwrap_or_default() {
        if let Some(sym) = run_git_command_with_timeout(
            &["symbolic-ref", "--quiet", &format!("refs/remotes/{remote}/HEAD")], cwd
        ).await.filter(|o| o.status.success())
         .and_then(|o| String::from_utf8(o.stdout).ok()) {
            if let Some((_, name)) = sym.trim().rsplit_once('/') {
                return Some(name.to_string());
            }
        }

        if let Some(show) = run_git_command_with_timeout(&["remote", "show", &remote], cwd).await
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok()) {
            for line in show.lines().map(str::trim) {
                if let Some(rest) = line.strip_prefix("HEAD branch:") {
                    let name = rest.trim();
                    if !name.is_empty() { return Some(name.to_string()); }
                }
            }
        }
    }

    for candidate in ["main", "master"] {
        if run_git_command_with_timeout(
            &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{candidate}")], cwd
        ).await.is_some_and(|o| o.status.success()) {
            return Some(candidate.to_string());
        }
    }
    None
}
```

- Build branch ancestry with de-dup and branches that contain HEAD.
```rust
use std::collections::HashSet;

async fn branch_ancestry(cwd: &std::path::Path) -> Option<Vec<String>> {
    let current = run_git_command_with_timeout(&["rev-parse", "--abbrev-ref", "HEAD"], cwd).await
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| s != "HEAD");

    let default = get_default_branch(cwd).await;
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    if let Some(cb) = current.clone() { if seen.insert(cb.clone()) { out.push(cb); } }
    if let Some(db) = default { if seen.insert(db.clone()) { out.push(db); } }

    for remote in get_git_remotes(cwd).await.unwrap_or_default() {
        let pattern = format!("refs/remotes/{remote}");
        if let Some(o) = run_git_command_with_timeout(&["for-each-ref", "--format=%(refname:short)", "--contains=HEAD", &pattern], cwd).await
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok()) {
            for line in o.lines().map(str::trim) {
                if let Some(branch) = line.strip_prefix(&format!("{remote}/")) {
                    if !branch.is_empty() && seen.insert(branch.to_string()) {
                        out.push(branch.to_string());
                    }
                }
            }
        }
    }

    Some(out)
}
```

- Compute closest remote SHA by minimal distance; fall back from local to remote ref.
```rust
use codex_protocol::mcp_protocol::GitSha;

async fn branch_remote_and_distance(
    cwd: &std::path::Path, branch: &str, remotes: &[String]
) -> Option<(Option<GitSha>, usize)> {
    let mut remote_sha = None;
    let mut remote_ref = None;

    for remote in remotes {
        let rr = format!("refs/remotes/{remote}/{branch}");
        let Some(v) = run_git_command_with_timeout(&["rev-parse", "--verify", "--quiet", &rr], cwd).await else {
            return None;
        };
        if v.status.success() {
            let sha = String::from_utf8(v.stdout).ok()?.trim().to_string();
            remote_sha = Some(GitSha::new(&sha));
            remote_ref = Some(rr);
            break;
        }
    }

    let count = if let Some(local) = run_git_command_with_timeout(&["rev-list", "--count", &format!("{branch}..HEAD")], cwd).await {
        if local.status.success() { local } else {
            let rr = remote_ref.as_ref()?;
            run_git_command_with_timeout(&["rev-list", "--count", &format!("{rr}..HEAD")], cwd).await?
        }
    } else {
        let rr = remote_ref.as_ref()?;
        run_git_command_with_timeout(&["rev-list", "--count", &format!("{rr}..HEAD")], cwd).await?
    };

    if !count.status.success() { return None; }
    let dist = String::from_utf8(count.stdout).ok()?.trim().parse().ok()?;
    Some((remote_sha, dist))
}
```

- Treat `git diff` exit codes 0 and 1 as success; document it.
```rust
async fn diff_against_sha(cwd: &std::path::Path, sha: &GitSha) -> Option<String> {
    let o = run_git_command_with_timeout(&["diff", &sha.0], cwd).await?;
    // 0 = no diff, 1 = diff present (both are "success" states for `git diff`)
    let ok = o.status.code().is_some_and(|c| c == 0 || c == 1);
    if !ok { return None; }

    String::from_utf8(o.stdout).ok()
}
```

- Include untracked files via `--no-index` and parallelize per-file diffs.
```rust
use futures::future::join_all;

async fn append_untracked_diffs(mut diff: String, cwd: &std::path::Path) -> Option<String> {
    let o = run_git_command_with_timeout(&["ls-files", "--others", "--exclude-standard"], cwd).await?;
    if !o.status.success() { return Some(diff); }

    let untracked: Vec<String> = String::from_utf8(o.stdout).ok()?
        .lines().map(|s| s.to_string()).filter(|s| !s.is_empty()).collect();

    let futs = untracked.into_iter().map(|file| async move {
        run_git_command_with_timeout(&["diff", "--binary", "--no-index", "/dev/null", &file], cwd).await
    });
    for extra in join_all(futs).await.into_iter().flatten() {
        if extra.status.code().is_some_and(|c| c == 0 || c == 1) {
            if let Ok(s) = String::from_utf8(extra.stdout) { diff.push_str(&s); }
        }
    }
    Some(diff)
}
```

- Add explicit protocol types for requests and responses.
```rust
// protocol/src/mcp_protocol.rs
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct GitDiffToRemoteParams { pub cwd: std::path::PathBuf }

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct GitDiffToRemoteResponse { pub sha: GitSha, pub diff: String }

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[serde(tag = "method", rename_all = "camelCase")]
pub enum ClientRequest {
    // ...
    GitDiffToRemote { #[serde(rename = "id")] request_id: mcp_types::RequestId, params: GitDiffToRemoteParams },
}
```

- Improve readability: use early returns and add a blank line after them; always inline variables in `format!`.
```rust
fn parse_remote_head(symref: &str) -> Option<String> {
    if symref.is_empty() {
        return None;
    }

    let trimmed = symref.trim();
    trimmed.rsplit_once('/').map(|(_, name)| name.to_string())
}

let rr = format!("refs/remotes/{remote}/{branch}");
```


**DON’Ts**
- Don’t use `String` for SHAs when a strong type exists.
```rust
// Don't
struct GitDiffToRemote { sha: String, diff: String }

// Do
struct GitDiffToRemote { sha: GitSha, diff: String }
```

- Don’t scatter git env overrides across tests/callers.
```rust
// Don't
Command::new("git").env("GIT_CONFIG_GLOBAL", "/dev/null"); // in multiple places

// Do (centralized in run_git_command_with_timeout)
```

- Don’t assume the default branch is `main` or `master` without checks.
```rust
// Don't
let default = "main".to_string();

// Do
let default = get_default_branch(cwd).await;
```

- Don’t ignore remote precedence; always prioritize `origin`.
```rust
// Don't
let remotes: Vec<String> = parse_stdout();

// Do
let mut remotes = parse_stdout();
if let Some(pos) = remotes.iter().position(|r| r == "origin") {
    let origin = remotes.remove(pos);
    remotes.insert(0, origin);
}
```

- Don’t treat `git diff` exit code 1 as failure.
```rust
// Don't
if !output.status.success() { return None; }

// Do
let ok = output.status.code().is_some_and(|c| c == 0 || c == 1);
if !ok { return None; }
```

- Don’t compute distances only against local branches; fall back to remote refs.
```rust
// Don't
let out = run_git(&["rev-list", "--count", &format!("{branch}..HEAD")]).await?;

// Do
let out = if local_failed {
    run_git(&["rev-list", "--count", &format!("{remote_ref}..HEAD")]).await?
} else { local_out };
```

- Don’t process untracked file diffs serially.
```rust
// Don't
for f in files { run_git(&["diff", "--no-index", "/dev/null", &f]).await; }

// Do
let futs = files.iter().map(|f| run_untracked(f));
let results = join_all(futs).await;
```

- Don’t cram dense logic without whitespace; add a blank line after early returns.
```rust
// Don't
if !cond { return None; } let next = work();

// Do
if !cond {
    return None;
}

let next = work();
```