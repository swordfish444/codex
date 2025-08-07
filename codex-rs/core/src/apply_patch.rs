use crate::codex::Session;
use crate::models::FunctionCallOutputPayload;
use crate::models::ResponseInputItem;
use crate::protocol::DiffHunk;
use crate::protocol::DiffLine;
use crate::protocol::DiffLineKind;
use crate::protocol::FileChange;
use crate::protocol::ReviewDecision;
use crate::safety::SafetyCheck;
use crate::safety::assess_patch_safety;
use codex_apply_patch::ApplyPatchAction;
use codex_apply_patch::ApplyPatchFileChange;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

pub const CODEX_APPLY_PATCH_ARG1: &str = "--codex-run-as-apply-patch";

pub(crate) enum InternalApplyPatchInvocation {
    /// The `apply_patch` call was handled programmatically, without any sort
    /// of sandbox, because the user explicitly approved it. This is the
    /// result to use with the `shell` function call that contained `apply_patch`.
    Output(ResponseInputItem),

    /// The `apply_patch` call was approved, either automatically because it
    /// appears that it should be allowed based on the user's sandbox policy
    /// *or* because the user explicitly approved it. In either case, we use
    /// exec with [`CODEX_APPLY_PATCH_ARG1`] to realize the `apply_patch` call,
    /// but [`ApplyPatchExec::auto_approved`] is used to determine the sandbox
    /// used with the `exec()`.
    DelegateToExec(ApplyPatchExec),
}

pub(crate) struct ApplyPatchExec {
    pub(crate) action: ApplyPatchAction,
    pub(crate) user_explicitly_approved_this_action: bool,
}

impl From<ResponseInputItem> for InternalApplyPatchInvocation {
    fn from(item: ResponseInputItem) -> Self {
        InternalApplyPatchInvocation::Output(item)
    }
}

pub(crate) async fn apply_patch(
    sess: &Session,
    sub_id: &str,
    call_id: &str,
    action: ApplyPatchAction,
) -> InternalApplyPatchInvocation {
    let writable_roots_snapshot = {
        #[allow(clippy::unwrap_used)]
        let guard = sess.writable_roots.lock().unwrap();
        guard.clone()
    };

    match assess_patch_safety(
        &action,
        sess.approval_policy,
        &writable_roots_snapshot,
        &sess.cwd,
    ) {
        SafetyCheck::AutoApprove { .. } => {
            InternalApplyPatchInvocation::DelegateToExec(ApplyPatchExec {
                action,
                user_explicitly_approved_this_action: false,
            })
        }
        SafetyCheck::AskUser => {
            // Compute a readable summary of path changes to include in the
            // approval request so the user can make an informed decision.
            //
            // Note that it might be worth expanding this approval request to
            // give the user the option to expand the set of writable roots so
            // that similar patches can be auto-approved in the future during
            // this session.
            let rx_approve = sess
                .request_patch_approval(sub_id.to_owned(), call_id.to_owned(), &action, None, None)
                .await;
            match rx_approve.await.unwrap_or_default() {
                ReviewDecision::Approved | ReviewDecision::ApprovedForSession => {
                    InternalApplyPatchInvocation::DelegateToExec(ApplyPatchExec {
                        action,
                        user_explicitly_approved_this_action: true,
                    })
                }
                ReviewDecision::Denied | ReviewDecision::Abort => {
                    ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.to_owned(),
                        output: FunctionCallOutputPayload {
                            content: "patch rejected by user".to_string(),
                            success: Some(false),
                        },
                    }
                    .into()
                }
            }
        }
        SafetyCheck::Reject { reason } => ResponseInputItem::FunctionCallOutput {
            call_id: call_id.to_owned(),
            output: FunctionCallOutputPayload {
                content: format!("patch rejected: {reason}"),
                success: Some(false),
            },
        }
        .into(),
    }
}

pub(crate) fn convert_apply_patch_to_protocol(
    action: &ApplyPatchAction,
) -> HashMap<PathBuf, FileChange> {
    let changes = action.changes();
    let mut result = HashMap::with_capacity(changes.len());
    for (path, change) in changes {
        let protocol_change = match change {
            ApplyPatchFileChange::Add { content } => FileChange::Add {
                content: content.clone(),
            },
            ApplyPatchFileChange::Delete => FileChange::Delete,
            ApplyPatchFileChange::Update {
                unified_diff,
                move_path,
                new_content: _new_content,
            } => {
                let hunks = parse_unified_diff_to_hunks(unified_diff).unwrap_or_default();
                FileChange::Update {
                    unified_diff: unified_diff.clone(),
                    move_path: move_path.clone(),
                    hunks: if hunks.is_empty() { None } else { Some(hunks) },
                }
            }
        };
        result.insert(path.clone(), protocol_change);
    }
    result
}

/// Parse a unified diff string into structured hunks. The input is expected to
/// contain one or more hunk headers (lines starting with "@@") followed by hunk
/// bodies. Lines starting with '+++', '---' (file headers) are ignored.
fn parse_unified_diff_to_hunks(src: &str) -> Option<Vec<DiffHunk>> {
    let mut hunks: Vec<DiffHunk> = Vec::new();
    let mut cur: Option<DiffHunk> = None;

    for line in src.lines() {
        if line.starts_with("---") || line.starts_with("+++") {
            // File headers: ignore.
            continue;
        }

        if let Some((old_start, old_count, new_start, new_count)) = parse_hunk_header(line) {
            // Flush previous hunk
            if let Some(h) = cur.take() {
                hunks.push(h);
            }
            cur = Some(DiffHunk {
                old_start: old_start as u32,
                old_count: old_count as u32,
                new_start: new_start as u32,
                new_count: new_count as u32,
                lines: Vec::new(),
            });
            continue;
        }

        if let Some(h) = cur.as_mut() {
            // Classify by prefix; store text without the prefix when present.
            let (kind, text) = if let Some(rest) = line.strip_prefix('+') {
                (DiffLineKind::Add, rest.to_string())
            } else if let Some(rest) = line.strip_prefix('-') {
                (DiffLineKind::Delete, rest.to_string())
            } else if let Some(rest) = line.strip_prefix(' ') {
                (DiffLineKind::Context, rest.to_string())
            } else {
                // Non-standard line inside hunk; keep as context with full text.
                (DiffLineKind::Context, line.to_string())
            };
            h.lines.push(DiffLine { kind, text });
        }
    }

    if let Some(h) = cur.take() {
        hunks.push(h);
    }

    Some(hunks)
}

// Lightweight parsing of a unified diff hunk header of the form:
// @@ -oldStart,oldCount +newStart,newCount @@
// Counts may be omitted which implies 1.
fn parse_hunk_header(line: &str) -> Option<(u64, u64, u64, u64)> {
    if !line.starts_with("@@") {
        return None;
    }
    let bytes = line.as_bytes();
    let mut i = 2usize;
    // skip spaces
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'-' {
        return None;
    }
    i += 1;
    let (old_start, c1) = parse_uint(&bytes[i..]);
    if c1 == 0 {
        return None;
    }
    i += c1;
    let mut old_count = 1u64;
    if i < bytes.len() && bytes[i] == b',' {
        i += 1;
        let (n, c) = parse_uint(&bytes[i..]);
        if c == 0 {
            return None;
        }
        old_count = n;
        i += c;
    }
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'+' {
        return None;
    }
    i += 1;
    let (new_start, c2) = parse_uint(&bytes[i..]);
    if c2 == 0 {
        return None;
    }
    i += c2;
    let mut new_count = 1u64;
    if i < bytes.len() && bytes[i] == b',' {
        i += 1;
        let (n, c) = parse_uint(&bytes[i..]);
        if c == 0 {
            return None;
        }
        new_count = n;
        i += c;
    }
    Some((old_start, old_count, new_start, new_count))
}

fn parse_uint(s: &[u8]) -> (u64, usize) {
    let mut i = 0usize;
    let mut n: u64 = 0;
    while i < s.len() {
        let b = s[i];
        if b.is_ascii_digit() {
            n = n * 10 + (b - b'0') as u64;
            i += 1;
        } else {
            break;
        }
    }
    (n, i)
}

pub(crate) fn get_writable_roots(cwd: &Path) -> Vec<PathBuf> {
    let mut writable_roots = Vec::new();
    if cfg!(target_os = "macos") {
        // On macOS, $TMPDIR is private to the user.
        writable_roots.push(std::env::temp_dir());

        // Allow pyenv to update its shims directory. Without this, any tool
        // that happens to be managed by `pyenv` will fail with an error like:
        //
        //   pyenv: cannot rehash: $HOME/.pyenv/shims isn't writable
        //
        // which is emitted every time `pyenv` tries to run `rehash` (for
        // example, after installing a new Python package that drops an entry
        // point). Although the sandbox is intentionally read‑only by default,
        // writing to the user's local `pyenv` directory is safe because it
        // is already user‑writable and scoped to the current user account.
        if let Ok(home_dir) = std::env::var("HOME") {
            let pyenv_dir = PathBuf::from(home_dir).join(".pyenv");
            writable_roots.push(pyenv_dir);
        }
    }

    writable_roots.push(cwd.to_path_buf());

    writable_roots
}
