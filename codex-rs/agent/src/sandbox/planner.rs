use std::collections::HashSet;
use std::path::Path;

use codex_apply_patch::ApplyPatchAction;
use codex_protocol::protocol::AskForApproval;
use codex_protocol::protocol::SandboxPolicy;

use crate::safety::SafetyCheck;
use crate::safety::assess_command_safety;
use crate::safety::assess_patch_safety;

use super::SandboxType;

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

pub struct CommandPlanRequest<'a> {
    pub command: &'a [String],
    pub approval: AskForApproval,
    pub policy: &'a SandboxPolicy,
    pub approved_session_commands: &'a HashSet<Vec<String>>,
    pub with_escalated_permissions: bool,
    pub justification: Option<&'a String>,
}

pub struct PatchPlanRequest<'a> {
    pub action: &'a ApplyPatchAction,
    pub approval: AskForApproval,
    pub policy: &'a SandboxPolicy,
    pub cwd: &'a Path,
    pub user_explicitly_approved: bool,
}

pub fn plan_exec(req: &CommandPlanRequest<'_>) -> ExecPlan {
    let safety = assess_command_safety(
        req.command,
        req.approval,
        req.policy,
        req.approved_session_commands,
        req.with_escalated_permissions,
    );

    match safety {
        SafetyCheck::AutoApprove { sandbox_type } => ExecPlan::approved(
            sandbox_type,
            should_escalate_on_failure(req.approval, sandbox_type),
            false,
        ),
        SafetyCheck::AskUser => ExecPlan::AskUser {
            reason: req.justification.map(ToOwned::to_owned),
        },
        SafetyCheck::Reject { reason } => ExecPlan::Reject { reason },
    }
}

pub fn plan_apply_patch(req: &PatchPlanRequest<'_>) -> ExecPlan {
    if req.user_explicitly_approved {
        return ExecPlan::approved(SandboxType::None, false, true);
    }

    match assess_patch_safety(req.action, req.approval, req.policy, req.cwd) {
        SafetyCheck::AutoApprove { sandbox_type } => ExecPlan::approved(
            sandbox_type,
            should_escalate_on_failure(req.approval, sandbox_type),
            false,
        ),
        SafetyCheck::AskUser => ExecPlan::AskUser { reason: None },
        SafetyCheck::Reject { reason } => ExecPlan::Reject { reason },
    }
}

pub fn should_escalate_on_failure(approval: AskForApproval, sandbox: SandboxType) -> bool {
    matches!(
        (approval, sandbox),
        (
            AskForApproval::UnlessTrusted | AskForApproval::OnFailure,
            SandboxType::MacosSeatbelt | SandboxType::LinuxSeccomp
        )
    )
}
