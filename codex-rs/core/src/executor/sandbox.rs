use crate::CODEX_APPLY_PATCH_ARG1;
use crate::apply_patch::ApplyPatchExec;
use crate::codex::Session;
use crate::exec::ExecParams;
use crate::exec::SandboxType;
use crate::executor::ExecError;
use crate::executor::ExecutionMode;
use crate::executor::ExecutionRequest;
use crate::executor::ExecutorConfig;
use crate::function_tool::FunctionCallError;
use crate::safety::SafetyCheck;
use crate::safety::assess_command_safety;
use crate::safety::assess_patch_safety;
use codex_protocol::protocol::AskForApproval;
use codex_protocol::protocol::ReviewDecision;
use std::collections::HashMap;
use std::collections::HashSet;
use std::env;

/// Sandbox placement options selected for an execution run, including whether
/// to escalate after failures and whether approvals should persist.
pub(crate) struct SandboxDecision {
    pub(crate) initial_sandbox: SandboxType,
    pub(crate) escalate_on_failure: bool,
    pub(crate) record_session_approval: bool,
}

impl SandboxDecision {
    fn auto(sandbox: SandboxType, escalate_on_failure: bool) -> Self {
        Self {
            initial_sandbox: sandbox,
            escalate_on_failure,
            record_session_approval: false,
        }
    }

    fn user_override(record_session_approval: bool) -> Self {
        Self {
            initial_sandbox: SandboxType::None,
            escalate_on_failure: false,
            record_session_approval,
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

/// Builds the command-line invocation that shells out to `codex apply_patch`
/// using the provided apply-patch request details.
pub(crate) fn build_exec_params_for_apply_patch(
    exec: &ApplyPatchExec,
    original: &ExecParams,
) -> Result<ExecParams, FunctionCallError> {
    let path_to_codex = env::current_exe()
        .ok()
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| {
            FunctionCallError::RespondToModel(
                "failed to determine path to codex executable".to_string(),
            )
        })?;

    let patch = exec.action.patch.clone();
    Ok(ExecParams {
        command: vec![path_to_codex, CODEX_APPLY_PATCH_ARG1.to_string(), patch],
        cwd: exec.action.cwd.clone(),
        timeout_ms: original.timeout_ms,
        // Run apply_patch with a minimal environment for determinism and to
        // avoid leaking host environment variables into the patch process.
        env: HashMap::new(),
        with_escalated_permissions: original.with_escalated_permissions,
        justification: original.justification.clone(),
    })
}

/// Determines how a command should be sandboxed, prompting the user when
/// policy requires explicit approval.
pub async fn select_sandbox(
    request: &ExecutionRequest,
    approval_policy: AskForApproval,
    approval_cache: HashSet<Vec<String>>,
    config: &ExecutorConfig,
    session: &Session,
    sub_id: &str,
    call_id: &str,
) -> Result<SandboxDecision, ExecError> {
    match &request.mode {
        ExecutionMode::Shell => {
            select_shell_sandbox(
                request,
                approval_policy,
                approval_cache,
                config,
                session,
                sub_id,
                call_id,
            )
            .await
        }
        ExecutionMode::ApplyPatch(exec) => {
            select_apply_patch_sandbox(exec, approval_policy, config)
        }
    }
}

async fn select_shell_sandbox(
    request: &ExecutionRequest,
    approval_policy: AskForApproval,
    approved_snapshot: HashSet<Vec<String>>,
    config: &ExecutorConfig,
    session: &Session,
    sub_id: &str,
    call_id: &str,
) -> Result<SandboxDecision, ExecError> {
    let command_for_safety = if request.approval_command.is_empty() {
        request.params.command.clone()
    } else {
        request.approval_command.clone()
    };

    let safety = assess_command_safety(
        &command_for_safety,
        approval_policy,
        &config.sandbox_policy,
        &approved_snapshot,
        request.params.with_escalated_permissions.unwrap_or(false),
    );

    match safety {
        SafetyCheck::AutoApprove { sandbox_type } => Ok(SandboxDecision::auto(
            sandbox_type,
            should_escalate_on_failure(approval_policy, sandbox_type),
        )),
        SafetyCheck::AskUser => {
            let decision = session
                .request_command_approval(
                    sub_id.to_string(),
                    call_id.to_string(),
                    request.approval_command.clone(),
                    request.params.cwd.clone(),
                    request.params.justification.clone(),
                )
                .await;

            match decision {
                ReviewDecision::Approved => Ok(SandboxDecision::user_override(false)),
                ReviewDecision::ApprovedForSession => Ok(SandboxDecision::user_override(true)),
                ReviewDecision::Denied | ReviewDecision::Abort => {
                    Err(ExecError::rejection("exec command rejected by user"))
                }
            }
        }
        SafetyCheck::Reject { reason } => Err(ExecError::rejection(format!(
            "exec command rejected: {reason}"
        ))),
    }
}

fn select_apply_patch_sandbox(
    exec: &ApplyPatchExec,
    approval_policy: AskForApproval,
    config: &ExecutorConfig,
) -> Result<SandboxDecision, ExecError> {
    if exec.user_explicitly_approved_this_action {
        return Ok(SandboxDecision::user_override(false));
    }

    match assess_patch_safety(
        &exec.action,
        approval_policy,
        &config.sandbox_policy,
        &config.sandbox_cwd,
    ) {
        SafetyCheck::AutoApprove { sandbox_type } => Ok(SandboxDecision::auto(
            sandbox_type,
            should_escalate_on_failure(approval_policy, sandbox_type),
        )),
        SafetyCheck::AskUser => Err(ExecError::rejection(
            "patch requires approval but none was recorded",
        )),
        SafetyCheck::Reject { reason } => {
            Err(ExecError::rejection(format!("patch rejected: {reason}")))
        }
    }
}
