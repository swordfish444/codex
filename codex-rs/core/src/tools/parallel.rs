use std::sync::Arc;

use tokio::task::JoinHandle;

use crate::codex::Session;
use crate::codex::TurnContext;
use crate::error::CodexErr;
use crate::function_tool::FunctionCallError;
use crate::tools::context::SharedTurnDiffTracker;
use crate::tools::router::ToolCall;
use crate::tools::router::ToolRouter;
use codex_protocol::models::ResponseInputItem;

use crate::codex::ProcessedResponseItem;

struct PendingToolCall {
    index: usize,
    handle: JoinHandle<Result<ResponseInputItem, FunctionCallError>>,
}

pub(crate) struct ToolCallRuntime {
    router: Arc<ToolRouter>,
    session: Arc<Session>,
    turn_context: Arc<TurnContext>,
    tracker: SharedTurnDiffTracker,
    sub_id: String,
    pending_calls: Vec<PendingToolCall>,
    serial_mode: bool,
}

impl ToolCallRuntime {
    pub(crate) fn new(
        router: Arc<ToolRouter>,
        session: Arc<Session>,
        turn_context: Arc<TurnContext>,
        tracker: SharedTurnDiffTracker,
        sub_id: String,
    ) -> Self {
        Self {
            router,
            session,
            turn_context,
            tracker,
            sub_id,
            pending_calls: Vec::new(),
            serial_mode: false,
        }
    }

    pub(crate) async fn handle_tool_call(
        &mut self,
        call: ToolCall,
        output_index: usize,
        output: &mut Vec<ProcessedResponseItem>,
    ) -> Result<Option<ResponseInputItem>, CodexErr> {
        let supports_parallel = self.router.tool_supports_parallel(&call.tool_name);
        if !self.serial_mode && supports_parallel {
            self.spawn_parallel(call, output_index);
            Ok(None)
        } else {
            if !supports_parallel {
                self.serial_mode = true;
            }
            if self.serial_mode && !self.pending_calls.is_empty() {
                self.resolve_pending(output.as_mut_slice()).await?;
            }
            self.dispatch_serial(call).await.map(Some)
        }
    }

    pub(crate) fn abort_all(&mut self) {
        while let Some(pending) = self.pending_calls.pop() {
            pending.handle.abort();
        }
    }

    pub(crate) async fn resolve_pending(
        &mut self,
        output: &mut [ProcessedResponseItem],
    ) -> Result<(), CodexErr> {
        while let Some(PendingToolCall { index, handle }) = self.pending_calls.pop() {
            match handle.await {
                Ok(Ok(response)) => {
                    if let Some(slot) = output.get_mut(index) {
                        slot.response = Some(response);
                    }
                }
                Ok(Err(FunctionCallError::Fatal(message))) => {
                    self.abort_all();
                    return Err(CodexErr::Fatal(message));
                }
                Ok(Err(other)) => {
                    self.abort_all();
                    return Err(CodexErr::Fatal(other.to_string()));
                }
                Err(join_err) => {
                    self.abort_all();
                    return Err(CodexErr::Fatal(format!(
                        "tool task failed to join: {join_err}"
                    )));
                }
            }
        }

        Ok(())
    }

    fn spawn_parallel(&mut self, call: ToolCall, index: usize) {
        let router = Arc::clone(&self.router);
        let session = Arc::clone(&self.session);
        let turn = Arc::clone(&self.turn_context);
        let tracker = Arc::clone(&self.tracker);
        let sub_id = self.sub_id.clone();
        let handle = tokio::spawn(async move {
            router
                .dispatch_tool_call(session, turn, tracker, sub_id, call)
                .await
        });
        self.pending_calls.push(PendingToolCall { index, handle });
    }

    async fn dispatch_serial(&self, call: ToolCall) -> Result<ResponseInputItem, CodexErr> {
        match self
            .router
            .dispatch_tool_call(
                Arc::clone(&self.session),
                Arc::clone(&self.turn_context),
                Arc::clone(&self.tracker),
                self.sub_id.clone(),
                call,
            )
            .await
        {
            Ok(response) => Ok(response),
            Err(FunctionCallError::Fatal(message)) => Err(CodexErr::Fatal(message)),
            Err(other) => Err(CodexErr::Fatal(other.to_string())),
        }
    }
}
