use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

use codex_apply_patch::ApplyPatchAction;
use codex_apply_patch::ApplyPatchFileChange;
use codex_protocol::protocol::AskForApproval;
use codex_protocol::protocol::FileChange;
use codex_protocol::protocol::ReviewDecision;
use codex_protocol::protocol::SandboxPolicy;

use crate::function_tool::FunctionCallError;
use crate::safety::SafetyCheck;
use crate::safety::assess_patch_safety;
use crate::services::ApprovalCoordinator;

pub const CODEX_APPLY_PATCH_ARG1: &str = "--codex-run-as-apply-patch";

pub enum InternalApplyPatchInvocation {
    /// The `apply_patch` call was handled programmatically, without any sort
    /// of sandbox, because the user explicitly approved it. This is the
    /// result to use with the `shell` function call that contained `apply_patch`.
    Output(Result<String, FunctionCallError>),

    /// The `apply_patch` call was approved, either automatically because it
    /// appears that it should be allowed based on the user's sandbox policy
    /// *or* because the user explicitly approved it. In either case, we use
    /// exec with [`CODEX_APPLY_PATCH_ARG1`] to realize the `apply_patch` call,
    /// but [`ApplyPatchExec::auto_approved`] is used to determine the sandbox
    /// used with the `exec()`.
    DelegateToExec(ApplyPatchExec),
}

#[derive(Debug)]
pub struct ApplyPatchExec {
    pub action: ApplyPatchAction,
    pub user_explicitly_approved_this_action: bool,
}

pub struct ApplyPatchContext<'a> {
    pub approval_policy: AskForApproval,
    pub sandbox_policy: &'a SandboxPolicy,
    pub cwd: &'a Path,
}

pub async fn apply_patch(
    approvals: &dyn ApprovalCoordinator,
    context: ApplyPatchContext<'_>,
    sub_id: &str,
    call_id: &str,
    action: ApplyPatchAction,
) -> InternalApplyPatchInvocation {
    match assess_patch_safety(
        &action,
        context.approval_policy,
        context.sandbox_policy,
        context.cwd,
    ) {
        SafetyCheck::AutoApprove { .. } => {
            InternalApplyPatchInvocation::DelegateToExec(ApplyPatchExec {
                action,
                user_explicitly_approved_this_action: false,
            })
        }
        SafetyCheck::AskUser => {
            let approval = approvals
                .request_patch_approval(sub_id.to_owned(), call_id.to_owned(), &action, None, None)
                .await;

            match approval {
                ReviewDecision::Approved | ReviewDecision::ApprovedForSession => {
                    InternalApplyPatchInvocation::DelegateToExec(ApplyPatchExec {
                        action,
                        user_explicitly_approved_this_action: true,
                    })
                }
                ReviewDecision::Denied | ReviewDecision::Abort => {
                    InternalApplyPatchInvocation::Output(Err(FunctionCallError::RespondToModel(
                        "patch rejected by user".to_string(),
                    )))
                }
            }
        }
        SafetyCheck::Reject { reason } => InternalApplyPatchInvocation::Output(Err(
            FunctionCallError::RespondToModel(format!("patch rejected: {reason}")),
        )),
    }
}

pub fn convert_apply_patch_to_protocol(action: &ApplyPatchAction) -> HashMap<PathBuf, FileChange> {
    let changes = action.changes();
    let mut result = HashMap::with_capacity(changes.len());
    for (path, change) in changes {
        let protocol_change = match change {
            ApplyPatchFileChange::Add { content } => FileChange::Add {
                content: content.clone(),
            },
            ApplyPatchFileChange::Delete { content } => FileChange::Delete {
                content: content.clone(),
            },
            ApplyPatchFileChange::Update {
                unified_diff,
                move_path,
                new_content: _new_content,
            } => FileChange::Update {
                unified_diff: unified_diff.clone(),
                move_path: move_path.clone(),
            },
        };
        result.insert(path.clone(), protocol_change);
    }
    result
}
