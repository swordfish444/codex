use std::sync::Arc;

use async_trait::async_trait;

use crate::apply_patch::ApplyPatchExec;
use crate::exec::ExecParams;
use crate::exec::ExecToolCallOutput;
use crate::executor::sandbox::build_exec_params_for_apply_patch;
use crate::function_tool::FunctionCallError;

pub(crate) enum ExecutionMode {
    Shell,
    ApplyPatch(ApplyPatchExec),
}

#[async_trait]
/// Backend-specific hooks that prepare and post-process execution requests for a
/// given [`ExecutionMode`].
pub(crate) trait ExecutionBackend: Send + Sync {
    fn prepare(
        &self,
        params: ExecParams,
        // Required for downcasting the apply_patch.
        mode: &ExecutionMode,
    ) -> Result<ExecParams, FunctionCallError>;

    async fn finalize(
        &self,
        output: ExecToolCallOutput,
        _mode: &ExecutionMode,
    ) -> Result<ExecToolCallOutput, FunctionCallError> {
        Ok(output)
    }
}

pub(crate) struct BackendStore {
    shell: Arc<dyn ExecutionBackend>,
    apply_patch: Arc<dyn ExecutionBackend>,
}

impl BackendStore {
    pub(crate) fn new() -> Self {
        Self {
            shell: Arc::new(ShellBackend),
            apply_patch: Arc::new(ApplyPatchBackend),
        }
    }

    pub(crate) fn for_mode(&self, mode: &ExecutionMode) -> Arc<dyn ExecutionBackend> {
        match mode {
            ExecutionMode::Shell => self.shell.clone(),
            ExecutionMode::ApplyPatch(_) => self.apply_patch.clone(),
        }
    }
}

pub(crate) fn default_backends() -> BackendStore {
    BackendStore::new()
}

struct ShellBackend;

#[async_trait]
impl ExecutionBackend for ShellBackend {
    fn prepare(
        &self,
        params: ExecParams,
        mode: &ExecutionMode,
    ) -> Result<ExecParams, FunctionCallError> {
        match mode {
            ExecutionMode::Shell => Ok(params),
            _ => Err(FunctionCallError::RespondToModel(
                "shell backend invoked with non-shell mode".to_string(),
            )),
        }
    }
}

struct ApplyPatchBackend;

#[async_trait]
impl ExecutionBackend for ApplyPatchBackend {
    fn prepare(
        &self,
        params: ExecParams,
        mode: &ExecutionMode,
    ) -> Result<ExecParams, FunctionCallError> {
        match mode {
            ExecutionMode::ApplyPatch(exec) => build_exec_params_for_apply_patch(exec, &params),
            ExecutionMode::Shell => Err(FunctionCallError::RespondToModel(
                "apply_patch backend invoked without patch context".to_string(),
            )),
        }
    }
}
