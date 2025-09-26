use std::collections::HashSet;
use std::path::Path;

use codex_apply_patch::ApplyPatchAction;

use crate::exec::ExecParams;
use crate::exec::SandboxType;
use crate::protocol::AskForApproval;
use crate::protocol::SandboxPolicy;
use crate::safety::SafetyCheck;
use crate::safety::assess_command_safety;
use crate::safety::assess_patch_safety;

#[derive(Clone, Debug)]
pub struct ExecRequest<'a> {
    pub params: &'a ExecParams,
    pub approval: AskForApproval,
    pub policy: &'a SandboxPolicy,
    pub approved_session_commands: &'a HashSet<Vec<String>>,
}

#[derive(Clone, Debug)]
pub enum ExecPlan {
    Reject {
        reason: String,
    },
    AskUser {
        reason: Option<String>,
    },
    Approved {
        sandbox: SandboxType,
        on_failure_escalate: bool,
        approved_by_user: bool,
    },
}

impl ExecPlan {
    pub fn approved(
        sandbox: SandboxType,
        on_failure_escalate: bool,
        approved_by_user: bool,
    ) -> Self {
        ExecPlan::Approved {
            sandbox,
            on_failure_escalate,
            approved_by_user,
        }
    }
}

pub fn plan_exec(req: &ExecRequest<'_>) -> ExecPlan {
    let params = req.params;
    let with_escalated_permissions = params.with_escalated_permissions.unwrap_or(false);
    let safety = assess_command_safety(
        &params.command,
        req.approval,
        req.policy,
        req.approved_session_commands,
        with_escalated_permissions,
    );

    match safety {
        SafetyCheck::AutoApprove { sandbox_type } => ExecPlan::Approved {
            sandbox: sandbox_type,
            on_failure_escalate: should_escalate_on_failure(req.approval, sandbox_type),
            approved_by_user: false,
        },
        SafetyCheck::AskUser => ExecPlan::AskUser {
            reason: params.justification.clone(),
        },
        SafetyCheck::Reject { reason } => ExecPlan::Reject { reason },
    }
}

#[derive(Clone, Debug)]
pub struct PatchExecRequest<'a> {
    pub action: &'a ApplyPatchAction,
    pub approval: AskForApproval,
    pub policy: &'a SandboxPolicy,
    pub cwd: &'a Path,
    pub user_explicitly_approved: bool,
}

pub fn plan_apply_patch(req: &PatchExecRequest<'_>) -> ExecPlan {
    if req.user_explicitly_approved {
        ExecPlan::Approved {
            sandbox: SandboxType::None,
            on_failure_escalate: false,
            approved_by_user: true,
        }
    } else {
        match assess_patch_safety(req.action, req.approval, req.policy, req.cwd) {
            SafetyCheck::AutoApprove { sandbox_type } => ExecPlan::Approved {
                sandbox: sandbox_type,
                on_failure_escalate: should_escalate_on_failure(req.approval, sandbox_type),
                approved_by_user: false,
            },
            SafetyCheck::AskUser => ExecPlan::AskUser { reason: None },
            SafetyCheck::Reject { reason } => ExecPlan::Reject { reason },
        }
    }
}

fn should_escalate_on_failure(approval: AskForApproval, sandbox: SandboxType) -> bool {
    matches!(
        (approval, sandbox),
        (
            AskForApproval::UnlessTrusted | AskForApproval::OnFailure,
            SandboxType::MacosSeatbelt | SandboxType::LinuxSeccomp
        )
    )
}
