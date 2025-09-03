#![allow(dead_code)]

use std::env;
use std::path::Path;
use std::path::PathBuf;

/// Patch classification used to choose normalization steps before applying.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchKind {
    /// Codex Patch format beginning with `*** Begin Patch`.
    CodexPatch,
    /// Unified diff that includes either `diff --git` headers or just `---/+++` file headers.
    GitUnified,
    /// Body contains `@@` hunks but lacks required file headers.
    HunkOnly,
    /// Unknown/unsupported format.
    Unknown,
}

/// How to handle whitespace in `git apply`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhitespaceMode {
    /// Default strict behavior.
    Strict,
    /// Equivalent to `--ignore-space-change`.
    IgnoreSpaceChange,
    /// Equivalent to `--whitespace=nowarn`.
    WhitespaceNowarn,
}

/// How to treat CRLF conversions in `git`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrlfMode {
    /// Use repo/user defaults.
    Default,
    /// Apply with `-c core.autocrlf=false -c core.safecrlf=false`.
    NoAutoCrlfNoSafe,
}

/// Context for an apply operation.
#[derive(Debug, Clone)]
pub struct ApplyContext {
    pub cwd: PathBuf,
    pub whitespace: WhitespaceMode,
    pub crlf_mode: CrlfMode,
}

/// High-level outcome of an apply attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyStatus {
    Success,
    Partial,
    Error,
}

/// Structured result produced by the apply runner.
#[derive(Debug, Clone)]
pub struct ApplyResult {
    pub status: ApplyStatus,
    pub changed_paths: Vec<String>,
    pub skipped_paths: Vec<String>,
    pub conflict_paths: Vec<String>,
    pub stdout_tail: String,
    pub stderr_tail: String,
    pub diagnostics: String,
}

/// Classify an incoming patch string by format.
pub fn classify_patch(s: &str) -> PatchKind {
    let t = s.trim_start();
    if t.starts_with("*** Begin Patch") {
        return PatchKind::CodexPatch;
    }
    // Unified diffs can be either full git style or just `---`/`+++` file headers.
    let has_diff_git = t.contains("\ndiff --git ") || t.starts_with("diff --git ");
    let has_dash_headers = t.contains("\n--- ") && t.contains("\n+++ ");
    let has_hunk = t.contains("\n@@ ") || t.starts_with("@@ ");
    if has_diff_git || (has_dash_headers && has_hunk) {
        return PatchKind::GitUnified;
    }
    if has_hunk {
        return PatchKind::HunkOnly;
    }
    PatchKind::Unknown
}

/// Build an `ApplyContext` from environment variables.
///
/// Supported envs:
/// - `CODEX_APPLY_WHITESPACE` = `ignore-space-change` | `whitespace-nowarn` | `strict` (default)
/// - `CODEX_APPLY_CRLF` = `no-autocrlf-nosafe` | `default` (default)
pub fn context_from_env(cwd: PathBuf) -> ApplyContext {
    let whitespace = match env::var("CODEX_APPLY_WHITESPACE").ok().as_deref() {
        Some("ignore-space-change") => WhitespaceMode::IgnoreSpaceChange,
        Some("whitespace-nowarn") => WhitespaceMode::WhitespaceNowarn,
        _ => WhitespaceMode::Strict,
    };
    let crlf_mode = match env::var("CODEX_APPLY_CRLF").ok().as_deref() {
        Some("no-autocrlf-nosafe") => CrlfMode::NoAutoCrlfNoSafe,
        _ => CrlfMode::Default,
    };
    ApplyContext {
        cwd,
        whitespace,
        crlf_mode,
    }
}

/// Main entry point for applying a patch. This will be implemented in subsequent steps.
pub fn apply_patch(patch: &str, ctx: &ApplyContext) -> ApplyResult {
    // Classify and convert if needed
    let kind = classify_patch(patch);
    let unified = match kind {
        PatchKind::GitUnified => patch.to_string(),
        PatchKind::CodexPatch => match convert_codex_patch_to_unified(patch, &ctx.cwd) {
            Ok(u) => u,
            Err(e) => {
                return ApplyResult {
                    status: ApplyStatus::Error,
                    changed_paths: Vec::new(),
                    skipped_paths: Vec::new(),
                    conflict_paths: Vec::new(),
                    stdout_tail: String::new(),
                    stderr_tail: String::new(),
                    diagnostics: format!("failed to convert codex patch to unified diff: {e}"),
                };
            }
        },
        PatchKind::HunkOnly | PatchKind::Unknown => {
            return ApplyResult {
                status: ApplyStatus::Error,
                changed_paths: Vec::new(),
                skipped_paths: Vec::new(),
                conflict_paths: Vec::new(),
                stdout_tail: String::new(),
                stderr_tail: String::new(),
                diagnostics: format!(
                    "unsupported patch format: {kind:?}; need unified diff with file headers"
                ),
            };
        }
    };

    apply_unified(&unified, ctx)
}

fn apply_unified(unified_patch: &str, ctx: &ApplyContext) -> ApplyResult {
    // 1) Ensure `git` exists
    if let Err(e) = run_git(&ctx.cwd, &[], &["--version"]) {
        return ApplyResult {
            status: ApplyStatus::Error,
            changed_paths: Vec::new(),
            skipped_paths: Vec::new(),
            conflict_paths: Vec::new(),
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            diagnostics: format!("git not available: {e}"),
        };
    }
    // 2) Determine repo root
    let repo_root = match run_git_capture(&ctx.cwd, &[], &["rev-parse", "--show-toplevel"]) {
        Ok(out) if out.status == 0 => out.stdout.trim().to_string(),
        Ok(out) => {
            return ApplyResult {
                status: ApplyStatus::Error,
                changed_paths: Vec::new(),
                skipped_paths: Vec::new(),
                conflict_paths: Vec::new(),
                stdout_tail: String::new(),
                stderr_tail: String::new(),
                diagnostics: format!(
                    "not a git repository (exit {}): {}",
                    out.status,
                    tail(&out.stderr)
                ),
            };
        }
        Err(e) => {
            return ApplyResult {
                status: ApplyStatus::Error,
                changed_paths: Vec::new(),
                skipped_paths: Vec::new(),
                conflict_paths: Vec::new(),
                stdout_tail: String::new(),
                stderr_tail: String::new(),
                diagnostics: format!("git rev-parse failed: {e}"),
            };
        }
    };

    // 3) Temp file
    let mut patch_path = std::env::temp_dir();
    patch_path.push(format!("codex-apply-{}.diff", std::process::id()));
    if let Err(e) = std::fs::write(&patch_path, unified_patch) {
        return ApplyResult {
            status: ApplyStatus::Error,
            changed_paths: Vec::new(),
            skipped_paths: Vec::new(),
            conflict_paths: Vec::new(),
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            diagnostics: format!("failed to write temp patch: {e}"),
        };
    }
    struct TempPatch(PathBuf);
    impl Drop for TempPatch {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let _guard = TempPatch(patch_path.clone());

    // 4) Preflight --check
    let mut preflight_args: Vec<&str> = vec!["apply", "--check"];
    push_whitespace_flags(&mut preflight_args, ctx.whitespace);
    // Compute a shell-friendly representation of the preflight command for logging.
    let preflight_cfg = crlf_cfg(ctx.crlf_mode);
    let preflight_cmd = render_command_for_log(
        &repo_root,
        &preflight_cfg,
        &prepend(&preflight_args, patch_path.to_string_lossy().as_ref()),
    );
    let preflight = run_git_capture(
        Path::new(&repo_root),
        preflight_cfg.as_slice(),
        &prepend(&preflight_args, patch_path.to_string_lossy().as_ref()),
    );
    if let Ok(out) = &preflight {
        if out.status != 0 {
            return ApplyResult {
                status: ApplyStatus::Error,
                changed_paths: Vec::new(),
                skipped_paths: Vec::new(),
                conflict_paths: Vec::new(),
                stdout_tail: tail(&out.stdout),
                stderr_tail: tail(&out.stderr),
                diagnostics: format!(
                    "git apply --check failed; working tree not modified; cmd: {preflight_cmd}"
                ),
            };
        }
    } else if let Err(e) = preflight {
        return ApplyResult {
            status: ApplyStatus::Error,
            changed_paths: Vec::new(),
            skipped_paths: Vec::new(),
            conflict_paths: Vec::new(),
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            diagnostics: format!("git apply --check failed to run: {e}; cmd: {preflight_cmd}"),
        };
    }

    // 5) Snapshot before
    let before = list_changed_paths(&repo_root);
    // 6) Apply
    let mut apply_args: Vec<&str> = vec!["apply", "--3way"];
    push_whitespace_flags(&mut apply_args, ctx.whitespace);
    let apply_cfg = crlf_cfg(ctx.crlf_mode);
    let apply_cmd = render_command_for_log(
        &repo_root,
        &apply_cfg,
        &prepend(&apply_args, patch_path.to_string_lossy().as_ref()),
    );
    let apply_out = run_git_capture(
        Path::new(&repo_root),
        apply_cfg.as_slice(),
        &prepend(&apply_args, patch_path.to_string_lossy().as_ref()),
    );
    let mut result = ApplyResult {
        status: ApplyStatus::Error,
        changed_paths: Vec::new(),
        skipped_paths: Vec::new(),
        conflict_paths: Vec::new(),
        stdout_tail: String::new(),
        stderr_tail: String::new(),
        diagnostics: String::new(),
    };
    match apply_out {
        Ok(out) => {
            result.stdout_tail = tail(&out.stdout);
            result.stderr_tail = tail(&out.stderr);
            result.conflict_paths = list_conflicts(&repo_root);
            let mut skipped = parse_skipped_paths(&result.stdout_tail);
            skipped.extend(parse_skipped_paths(&result.stderr_tail));
            skipped.sort();
            skipped.dedup();
            result.skipped_paths = skipped;
            let after = list_changed_paths(&repo_root);
            result.changed_paths = set_delta(&before, &after);
            result.status = if out.status == 0 {
                ApplyStatus::Success
            } else if !result.changed_paths.is_empty() || !result.conflict_paths.is_empty() {
                ApplyStatus::Partial
            } else {
                ApplyStatus::Error
            };
            result.diagnostics = format!(
                "git apply exit={} ({} changed, {} skipped, {} conflicts); cmd: {}",
                out.status,
                result.changed_paths.len(),
                result.skipped_paths.len(),
                result.conflict_paths.len(),
                apply_cmd
            );
        }
        Err(e) => {
            result.status = ApplyStatus::Error;
            result.diagnostics = format!("failed to run git apply: {e}; cmd: {apply_cmd}");
        }
    }
    result
}

fn render_command_for_log(cwd: &str, git_cfg: &[&str], args: &[&str]) -> String {
    fn quote(s: &str) -> String {
        let simple = s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.:/@%+".contains(c));
        if simple {
            s.to_string()
        } else {
            format!("'{}'", s.replace('\'', "'\\''"))
        }
    }
    let mut parts: Vec<String> = Vec::new();
    parts.push("git".to_string());
    for a in git_cfg {
        parts.push(quote(a));
    }
    for a in args {
        parts.push(quote(a));
    }
    format!("(cd {} && {})", quote(cwd), parts.join(" "))
}

fn convert_codex_patch_to_unified(patch: &str, cwd: &Path) -> Result<String, String> {
    // Parse codex patch and verify paths relative to cwd
    let argv = vec!["apply_patch".to_string(), patch.to_string()];
    let verified = codex_apply_patch::maybe_parse_apply_patch_verified(&argv, cwd);
    match verified {
        codex_apply_patch::MaybeApplyPatchVerified::Body(action) => {
            let mut parts: Vec<String> = Vec::new();
            for (abs_path, change) in action.changes() {
                let rel_path = abs_path.strip_prefix(cwd).unwrap_or(abs_path);
                let rel_str = rel_path.to_string_lossy();
                match change {
                    codex_apply_patch::ApplyPatchFileChange::Add { content } => {
                        let header = format!(
                            "diff --git a/{rel_str} b/{rel_str}
new file mode 100644
--- /dev/null
+++ b/{rel_str}
"
                        );
                        let body = build_add_hunk(content);
                        parts.push(format!("{header}{body}"));
                    }
                    codex_apply_patch::ApplyPatchFileChange::Delete { .. } => {
                        let header = format!(
                            "diff --git a/{rel_str} b/{rel_str}
deleted file mode 100644
--- a/{rel_str}
+++ /dev/null
"
                        );
                        parts.push(header);
                    }
                    codex_apply_patch::ApplyPatchFileChange::Update {
                        unified_diff,
                        move_path,
                        ..
                    } => {
                        let new_rel = move_path
                            .as_ref()
                            .map(|p| {
                                p.strip_prefix(cwd)
                                    .unwrap_or(p)
                                    .to_string_lossy()
                                    .to_string()
                            })
                            .unwrap_or_else(|| rel_str.to_string());
                        let header = format!(
                            "diff --git a/{rel_str} b/{new_rel}
--- a/{rel_str}
+++ b/{new_rel}
"
                        );
                        parts.push(format!("{header}{unified_diff}"));
                    }
                }
            }
            if parts.is_empty() {
                Err("empty patch after conversion".to_string())
            } else {
                Ok(parts.join("\n"))
            }
        }
        codex_apply_patch::MaybeApplyPatchVerified::CorrectnessError(e) => {
            Err(format!("patch correctness: {e}"))
        }
        codex_apply_patch::MaybeApplyPatchVerified::ShellParseError(e) => {
            Err(format!("shell parse: {e:?}"))
        }
        _ => Err("not an apply_patch payload".to_string()),
    }
}

fn build_add_hunk(content: &str) -> String {
    let norm = content.replace("\r\n", "\n");
    let mut lines: Vec<&str> = norm.split('\n').collect();
    if let Some("") = lines.last().copied() {
        lines.pop();
    }
    let count = lines.len();
    if count == 0 {
        return String::new();
    }
    let mut out = String::new();
    out.push_str(&format!("@@ -0,0 +1,{count} @@\n"));
    for l in lines {
        out.push('+');
        out.push_str(l);
        out.push('\n');
    }
    out
}

fn push_whitespace_flags(args: &mut Vec<&str>, mode: WhitespaceMode) {
    match mode {
        WhitespaceMode::Strict => {}
        WhitespaceMode::IgnoreSpaceChange => args.push("--ignore-space-change"),
        WhitespaceMode::WhitespaceNowarn => {
            args.push("--whitespace");
            args.push("nowarn");
        }
    }
}

fn crlf_cfg(mode: CrlfMode) -> Vec<&'static str> {
    match mode {
        CrlfMode::Default => vec![],
        CrlfMode::NoAutoCrlfNoSafe => {
            vec!["-c", "core.autocrlf=false", "-c", "core.safecrlf=false"]
        }
    }
}

fn prepend<'a>(base: &'a [&'a str], tail: &'a str) -> Vec<&'a str> {
    let mut v = base.to_vec();
    v.push(tail);
    v
}

struct GitOutput {
    status: i32,
    stdout: String,
    stderr: String,
}

fn run_git(cwd: &std::path::Path, git_cfg: &[&str], args: &[&str]) -> std::io::Result<()> {
    let status = std::process::Command::new("git")
        .args(git_cfg)
        .args(args)
        .current_dir(cwd)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "git {:?} exited {}",
            args,
            status.code().unwrap_or(-1)
        )))
    }
}

fn run_git_capture(
    cwd: &std::path::Path,
    git_cfg: &[&str],
    args: &[&str],
) -> std::io::Result<GitOutput> {
    let out = std::process::Command::new("git")
        .args(git_cfg)
        .args(args)
        .current_dir(cwd)
        .output()?;
    Ok(GitOutput {
        status: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

fn list_changed_paths(repo_root: &str) -> Vec<String> {
    let cwd = std::path::Path::new(repo_root);
    match run_git_capture(cwd, &[], &["diff", "--name-only"]) {
        Ok(out) if out.status == 0 => out
            .stdout
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn list_conflicts(repo_root: &str) -> Vec<String> {
    let cwd = std::path::Path::new(repo_root);
    match run_git_capture(cwd, &[], &["ls-files", "-u"]) {
        Ok(out) if out.status == 0 => {
            let mut set = std::collections::BTreeSet::new();
            for line in out.stdout.lines() {
                // format: <mode> <sha> <stage>\t<path>
                if let Some((_meta, path)) = line.split_once('\t') {
                    set.insert(path.trim().to_string());
                }
            }
            set.into_iter().collect()
        }
        _ => Vec::new(),
    }
}

fn parse_skipped_paths(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        // error: path/to/file.txt does not match index
        if let Some(rest) = l.strip_prefix("error:") {
            let rest = rest.trim();
            if let Some(p) = rest.strip_suffix("does not match index") {
                let p = p.trim().trim_end_matches(':').trim();
                if !p.is_empty() {
                    out.push(p.to_string());
                }
                continue;
            }
        }
        // patch failed: path/to/file.txt: content
        if let Some(rest) = l.strip_prefix("patch failed:") {
            let rest = rest.trim();
            if let Some((p, _)) = rest.split_once(':') {
                let p = p.trim();
                if !p.is_empty() {
                    out.push(p.to_string());
                }
            }
        }
    }
    out
}

fn tail(s: &str) -> String {
    const MAX: usize = 2000;
    if s.len() <= MAX {
        s.to_string()
    } else {
        s[s.len() - MAX..].to_string()
    }
}

fn set_delta(before: &[String], after: &[String]) -> Vec<String> {
    use std::collections::BTreeSet;
    let b: BTreeSet<_> = before.iter().collect();
    let a: BTreeSet<_> = after.iter().collect();
    a.difference(&b).map(|s| (*s).clone()).collect()
}

/// Synthesize a unified git diff for a single file from a bare hunk body.
pub fn synthesize_unified_single_file(hunk_body: &str, old_path: &str, new_path: &str) -> String {
    // Ensure body ends with newline
    let mut body = hunk_body.to_string();
    if !body.ends_with("\n") {
        body.push('\n');
    }
    format!(
        "diff --git a/{old_path} b/{new_path}
--- a/{old_path}
+++ b/{new_path}
{body}"
    )
}

/// Split a bare hunk body into per-file segments using a conservative delimiter.
/// We look for lines that equal "*** End of File" (as emitted by our apply-patch format)
/// and use that to separate bodies for multiple files.
pub fn split_hunk_body_into_files(body: &str) -> Vec<String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut cur = String::new();
    for line in body.lines() {
        if line.trim() == "*** End of File" {
            if !cur.is_empty() {
                cur.push('\n');
                chunks.push(cur);
                cur = String::new();
            }
        } else {
            cur.push_str(line);
            cur.push('\n');
        }
    }
    if !cur.trim().is_empty() {
        chunks.push(cur);
    }
    chunks
}
