use std::env;

use crate::apply_patch::ApplyPatchExec;
use crate::apply_patch::CODEX_APPLY_PATCH_ARG1;
use crate::exec::ExecParams;
use crate::function_tool::FunctionCallError;

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
        env: original.env.clone(),
        with_escalated_permissions: original.with_escalated_permissions,
        justification: original.justification.clone(),
    })
}
