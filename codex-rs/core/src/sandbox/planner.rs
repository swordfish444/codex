use std::collections::HashSet;
use std::path::Path;

use codex_agent::apply_patch::ApplyPatchExec;
use codex_agent::sandbox::CommandPlanRequest;
use codex_agent::sandbox::ExecPlan;
use codex_agent::sandbox::PatchPlanRequest;
use codex_agent::sandbox::SandboxType;
use codex_agent::sandbox::plan_apply_patch;
use codex_agent::sandbox::plan_exec;
use codex_agent::services::ApprovalCoordinator;

use crate::exec::ExecParams;
use crate::function_tool::FunctionCallError;
use crate::protocol::AskForApproval;
use crate::protocol::ReviewDecision;
use crate::protocol::SandboxPolicy;

#[derive(Debug)]
pub(crate) struct PreparedExec {
    pub(crate) params: ExecParams,
    pub(crate) plan: ExecPlan,
    pub(crate) command_for_display: Vec<String>,
    pub(crate) apply_patch_exec: Option<ApplyPatchExec>,
}

pub(crate) async fn prepare_exec_invocation(
    approvals: &dyn ApprovalCoordinator,
    approval_policy: AskForApproval,
    sandbox_policy: &SandboxPolicy,
    cwd: &Path,
    sub_id: &str,
    call_id: &str,
    params: ExecParams,
    apply_patch_exec: Option<ApplyPatchExec>,
    approved_session_commands: HashSet<Vec<String>>,
) -> Result<PreparedExec, FunctionCallError> {
    let command_for_display = if let Some(exec) = apply_patch_exec.as_ref() {
        vec!["apply_patch".to_string(), exec.action.patch.clone()]
    } else {
        params.command.clone()
    };

    let plan = if let Some(exec) = apply_patch_exec.as_ref() {
        let plan_req = PatchPlanRequest {
            action: &exec.action,
            approval: approval_policy,
            policy: sandbox_policy,
            cwd,
            user_explicitly_approved: exec.user_explicitly_approved_this_action,
        };

        match plan_apply_patch(&plan_req) {
            plan @ ExecPlan::Approved { .. } => plan,
            ExecPlan::AskUser { .. } => {
                return Err(FunctionCallError::RespondToModel(
                    "patch requires approval but none was recorded".to_string(),
                ));
            }
            ExecPlan::Reject { reason } => {
                return Err(FunctionCallError::RespondToModel(format!(
                    "patch rejected: {reason}"
                )));
            }
        }
    } else {
        let plan_req = CommandPlanRequest {
            command: &params.command,
            approval: approval_policy,
            policy: sandbox_policy,
            approved_session_commands: &approved_session_commands,
            with_escalated_permissions: params.with_escalated_permissions.unwrap_or(false),
            justification: params.justification.as_ref(),
        };

        match plan_exec(&plan_req) {
            plan @ ExecPlan::Approved { .. } => plan,
            ExecPlan::AskUser { reason } => {
                let decision = approvals
                    .request_command_approval(
                        sub_id.to_string(),
                        call_id.to_string(),
                        params.command.clone(),
                        params.cwd.clone(),
                        reason,
                    )
                    .await;

                match decision {
                    ReviewDecision::Approved => ExecPlan::approved(SandboxType::None, false, true),
                    ReviewDecision::ApprovedForSession => {
                        approvals.add_approved_command(params.command.clone()).await;
                        ExecPlan::approved(SandboxType::None, false, true)
                    }
                    ReviewDecision::Denied | ReviewDecision::Abort => {
                        return Err(FunctionCallError::RespondToModel(
                            "exec command rejected by user".to_string(),
                        ));
                    }
                }
            }
            ExecPlan::Reject { reason } => {
                return Err(FunctionCallError::RespondToModel(format!(
                    "exec command rejected: {reason:?}"
                )));
            }
        }
    };

    Ok(PreparedExec {
        params,
        plan,
        command_for_display,
        apply_patch_exec,
    })
}
