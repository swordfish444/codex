**DOs**

- **Derive, don’t cache diffs:** Recompute the turn’s unified diff on demand instead of storing it on the struct.
```rust
impl TurnDiffTracker {
    pub fn get_unified_diff(&mut self) -> anyhow::Result<Option<String>> {
        let mut out = String::new();
        for internal in self.sorted_internal_names() {
            out.push_str(self.get_file_diff(&internal).as_str());
            if !out.ends_with('\n') { out.push('\n'); }
        }
        Ok(if out.trim().is_empty() { None } else { Some(out) })
    }
}
```

- **Track baselines in memory:** Snapshot initial bytes/mode/OID the first time a path is touched; avoid temp dirs.
```rust
struct BaselineFileInfo {
    path: std::path::PathBuf,
    content: Vec<u8>,
    mode: FileMode,
    oid: String,
}

impl TurnDiffTracker {
    pub fn on_patch_begin(&mut self, changes: &std::collections::HashMap<PathBuf, FileChange>) {
        for (path, change) in changes {
            let internal = self.ensure_internal_name(path);
            if !self.baseline_file_info.contains_key(&internal) {
                let (content, mode, oid) = if path.exists() {
                    let mode = file_mode_for_path(path).unwrap_or(FileMode::Regular);
                    let content = blob_bytes(path, &mode).unwrap_or_default();
                    let oid = self.git_blob_oid_for_path(path)
                        .unwrap_or_else(|| format!("{:x}", git_blob_sha1_hex_bytes(&content)));
                    (content, mode, oid)
                } else {
                    (Vec::new(), FileMode::Regular, ZERO_OID.to_string())
                };
                self.baseline_file_info.insert(internal.clone(), BaselineFileInfo {
                    path: path.clone(), content, mode, oid
                });
            }
            if let FileChange::Update { move_path: Some(dest), .. } = change {
                self.remap_internal_name(path, dest);
            }
        }
    }
}
```

- **Use strong types for file mode:** Prefer an enum over String; implement Display for Git-style headers.
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FileMode { Regular, #[cfg(unix)] Executable, Symlink }

impl std::fmt::Display for FileMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            FileMode::Regular => "100644",
            #[cfg(unix)] FileMode::Executable => "100755",
            FileMode::Symlink => "120000",
        })
    }
}

#[cfg(unix)]
fn file_mode_for_path(p: &std::path::Path) -> Option<FileMode> {
    use std::os::unix::fs::PermissionsExt;
    let m = std::fs::symlink_metadata(p).ok()?;
    if m.file_type().is_symlink() { return Some(FileMode::Symlink); }
    Some(if (m.permissions().mode() & 0o111) != 0 { FileMode::Executable } else { FileMode::Regular })
}
```

- **Diff text with similar::TextDiff:** Prefer in-process diffs over shelling out to git.
```rust
let diff = similar::TextDiff::from_lines(left, right);
let unified = diff.unified_diff().context_radius(3)
    .header(old_header, new_header)
    .to_string();
out.push_str(&unified);
```

- **Normalize and relativize paths:** Show repo-relative, slash-normalized paths in headers.
```rust
fn relative_to_git_root_str(&mut self, path: &std::path::Path) -> String {
    let s = if let Some(root) = self.find_git_root_cached(path) {
        path.strip_prefix(&root).unwrap_or(path).display().to_string()
    } else { path.display().to_string() };
    s.replace('\\', "/")
}
```

- **Sort output for stability:** Emit per-file diffs in a deterministic order.
```rust
let mut names: Vec<String> = self.baseline_file_info.keys().cloned().collect();
names.sort_by_key(|n| self.get_path_for_internal(n)
    .map(|p| self.relative_to_git_root_str(&p)).unwrap_or_default());
```

- **Compute blob OIDs correctly:** Use git’s hash-object when available; otherwise compute the Git blob hash locally.
```rust
fn git_blob_sha1_hex_bytes(data: &[u8]) -> sha1::digest::Output<sha1::Sha1> {
    use sha1::Digest;
    let header = format!("blob {}\0", data.len());
    let mut h = sha1::Sha1::new(); h.update(header.as_bytes()); h.update(data); h.finalize()
}

let right_oid = if current_mode == FileMode::Symlink {
    format!("{:x}", git_blob_sha1_hex_bytes(&bytes))
} else {
    self.git_blob_oid_for_path(path).unwrap_or_else(|| format!("{:x}", git_blob_sha1_hex_bytes(&bytes)))
};
```

- **Handle symlinks by hashing targets (Unix):** Readlink to bytes for Git-compatible blob content.
```rust
#[cfg(unix)]
fn symlink_blob_bytes(p: &std::path::Path) -> Option<Vec<u8>> {
    use std::os::unix::ffi::OsStrExt;
    std::fs::read_link(p).ok().map(|t| t.as_os_str().as_bytes().to_vec())
}
```

- **Emit end-of-turn diff once:** After patch events and at the end of a turn, send a TurnDiffEvent if non-empty.
```rust
if let Ok(Some(unified_diff)) = turn_diff_tracker.get_unified_diff() {
    let _ = sess.tx_event.send(Event { id: sub_id.to_string(), msg: EventMsg::TurnDiff(TurnDiffEvent { unified_diff }) }).await;
}
```

- **Prefer breaking the loop to post-process:** Exit the event loop (e.g., on token usage) then publish final messages.
```rust
let token_usage = break_token_usage; // break out of stream loop
// ...after loop:
if let Ok(Some(unified_diff)) = turn_diff_tracker.get_unified_diff() {
    publish_turn_diff(unified_diff).await;
}
```

- **Write precise snapshot tests:** Normalize paths; assert exact diffs using raw strings.
```rust
let got = normalize(&tracker.get_unified_diff()?.unwrap(), tmp.path());
let want = r#"diff --git a/<TMP>/a.txt b/<TMP>/a.txt
new file mode 100644
index 0000000000000000000000000000000000000000..d3b07384d113edec49eaa6238ad5ff00
--- /dev/null
+++ b/<TMP>/a.txt
@@ -0,0 +1 @@
+foo
"#;
assert_eq!(got, want);
```

- **Keep Cargo versions aligned and tidy:** Match major versions across crates; sort dependencies.
```toml
# codex-rs/core/Cargo.toml
similar = "2"
tempfile = "3"
```

- **Plan for concurrency:** If parallel tool calls are expected, wrap shared trackers.
```rust
struct TurnContext {
    turn_diff: std::sync::Arc<tokio::sync::Mutex<TurnDiffTracker>>,
}
```


**DON’Ts**

- **Don’t store derived state:** Avoid fields like `unified_diff: Option<String>` on the tracker; derive it each call.
```rust
// Bad: mutable cached copy drifts out of sync
pub struct TurnDiffTracker { pub unified_diff: Option<String>, /* ... */ }
```

- **Don’t shell out to git for diffing:** `git diff --no-index` is slower, brittle, and env-dependent.
```bash
# Bad
git -c color.ui=false diff --no-index -- baseline current
```

- **Don’t represent modes as strings:** Using "100644"/"100755"/"120000" strings everywhere obscures intent.
```rust
// Bad
let mode: String = "100644".into();
```

- **Don’t ignore Results silently:** Avoid `let _ = ...;` when errors should be logged or propagated.
```rust
// Bad
let _ = turn_diff_tracker.on_patch_begin(&changes);
// Better
turn_diff_tracker.on_patch_begin(&changes);
```

- **Don’t bury early returns deep in loops:** Prefer `break` then perform finalization afterward.
```rust
// Bad: return from the middle of the event loop
return Ok(output);
```

- **Don’t tie tracker to a single async path:** Passing `&mut TurnDiffTracker` deeply can block parallel calls.
```rust
// Bad
async fn handle(..., tracker: &mut TurnDiffTracker) { /* ... */ }
```

- **Don’t assume absolute paths in diffs:** Emit repo-relative, slash-normalized headers for portability.
```text
# Bad
diff --git a/C:\repo\file.rs b/C:\repo\file.rs
```

- **Don’t fabricate internal names with extensions or spaces:** Internal identifiers should be opaque and stable.
```rust
// Bad
format!("{uuid}.{}", maybe_ext)
// Better
uuid.to_string()
```

- **Don’t rely on user git config:** If invoking git (e.g., hash-object), neutralize config.
```rust
use std::process::Command;
let mut cmd = Command::new("git");
cmd.env("GIT_CONFIG_GLOBAL", "/dev/null").env("GIT_CONFIG_NOSYSTEM", "1");
```

- **Don’t use substring checks in tests:** Validate exact unified diff output; use raw strings for readability.
```rust
// Bad
assert!(diff.contains("new file mode"));
// Better
assert_eq!(diff, expected);
```