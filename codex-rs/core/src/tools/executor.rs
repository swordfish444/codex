use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use futures::FutureExt;
use tokio::task::JoinSet;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::trace;
use tracing::warn;

use crate::codex::Session;
use crate::codex::TurnContext;
use crate::error::CodexErr;
use crate::error::Result as CodexResult;
use crate::event_mapping::map_response_item_to_event_messages;
use crate::function_tool::FunctionCallError;
use crate::protocol::Event;
use crate::tools::context::ToolPayload;
use crate::tools::router::ToolRouter;
use crate::tools::router::ToolCall;
use crate::turn_diff_tracker::TurnDiffTracker;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ResponseInputItem;
use codex_protocol::models::ResponseItem;

#[derive(Debug)]
pub(crate) struct ProcessedResponseItem {
    pub item: ResponseItem,
    pub response: Option<ResponseInputItem>,
}

pub(crate) struct ToolCallExecutor {
    router: Arc<ToolRouter>,
    session: Arc<Session>,
    turn_context: Arc<TurnContext>,
    allow_parallel_read_only: bool,
    read_only_tasks: JoinSet<(usize, Result<ResponseInputItem, FunctionCallError>)>,
    processed_items: Vec<ProcessedResponseItem>,
}

impl ToolCallExecutor {
    pub(crate) fn new(
        router: Arc<ToolRouter>,
        session: Arc<Session>,
        turn_context: Arc<TurnContext>,
    ) -> Self {
        let allow_parallel_read_only =
            router.has_read_only_tools() && turn_context.tools_config.enable_parallel_read_only;

        Self {
            router,
            session,
            turn_context,
            allow_parallel_read_only,
            read_only_tasks: JoinSet::new(),
            processed_items: Vec::new(),
        }
    }

    pub(crate) fn drain_ready(&mut self) -> CodexResult<()> {
        while let Some(res) = self.read_only_tasks.try_join_next() {
            match res {
                Ok((idx, response)) => self.assign_result(idx, response)?,
                Err(join_err) => {
                    warn!(
                        ?join_err,
                        "parallel read-only task aborted before completion"
                    );
                }
            }
        }
        Ok(())
    }

    pub(crate) async fn flush(&mut self) -> CodexResult<()> {
        while let Some(res) = self.read_only_tasks.join_next().await {
            match res {
                Ok((idx, response)) => self.assign_result(idx, response)?,
                Err(join_err) => {
                    warn!(
                        ?join_err,
                        "parallel read-only task aborted before completion"
                    );
                }
            }
        }
        Ok(())
    }

    pub(crate) async fn handle_output_item(
        &mut self,
        item: ResponseItem,
        turn_diff_tracker: &mut TurnDiffTracker,
        sub_id: &str,
    ) -> CodexResult<()> {
        match self
            .router
            .build_tool_call(self.session.as_ref(), item.clone())
        {
            Ok(Some(call)) => {
                let payload_preview = call.payload.log_payload().into_owned();
                info!("ToolCall: {} {}", call.tool_name, payload_preview);

                let idx = self.processed_items.len();
                self.processed_items.push(ProcessedResponseItem {
                    item,
                    response: None,
                });

                if self.allow_parallel_read_only && call.capabilities.read_only {
                    self.schedule_parallel_task(idx, call, sub_id);
                } else {
                    self.flush().await?;
                    let response = self
                        .router
                        .dispatch_tool_call(
                            self.session.as_ref(),
                            self.turn_context.as_ref(),
                            turn_diff_tracker,
                            sub_id,
                            call,
                        )
                        .await
                        .map_err(Self::map_dispatch_error)?;
                    if let Some(slot) = self.processed_items.get_mut(idx) {
                        slot.response = Some(response);
                    }
                }
            }
            Ok(None) => {
                self.emit_response_item_events(sub_id, &item).await?;
                self.processed_items.push(ProcessedResponseItem {
                    item,
                    response: None,
                });
            }
            Err(FunctionCallError::RespondToModel(msg)) => {
                if msg == "LocalShellCall without call_id or id" {
                    self.turn_context
                        .client
                        .get_otel_event_manager()
                        .log_tool_failed("local_shell", &msg);
                    error!(msg);
                }

                self.flush().await?;
                self.processed_items.push(ProcessedResponseItem {
                    item,
                    response: Some(ResponseInputItem::FunctionCallOutput {
                        call_id: String::new(),
                        output: FunctionCallOutputPayload {
                            content: msg,
                            success: None,
                        },
                    }),
                });
            }
            Err(FunctionCallError::MissingLocalShellCallId) => {
                let msg = "LocalShellCall without call_id or id";
                self.turn_context
                    .client
                    .get_otel_event_manager()
                    .log_tool_failed("local_shell", msg);
                error!(msg);

                self.flush().await?;
                self.processed_items.push(ProcessedResponseItem {
                    item,
                    response: Some(ResponseInputItem::FunctionCallOutput {
                        call_id: String::new(),
                        output: FunctionCallOutputPayload {
                            content: msg.to_string(),
                            success: None,
                        },
                    }),
                });
            }
            Err(err) => {
                self.flush().await?;
                return Err(Self::map_dispatch_error(err));
            }
        }

        Ok(())
    }

    fn assign_result(
        &mut self,
        idx: usize,
        response: Result<ResponseInputItem, FunctionCallError>,
    ) -> CodexResult<()> {
        match response {
            Ok(response) => {
                if let Some(slot) = self.processed_items.get_mut(idx) {
                    slot.response = Some(response);
                } else {
                    warn!(idx, "parallel tool completion missing output slot");
                }
                Ok(())
            }
            Err(err) => Err(Self::map_dispatch_error(err)),
        }
    }

    pub(crate) fn take_processed_items(mut self) -> CodexResult<Vec<ProcessedResponseItem>> {
        self.drain_ready()?;
        Ok(self.processed_items)
    }

    fn schedule_parallel_task(&mut self, idx: usize, call: ToolCall, sub_id: &str) {
        let router_for_task = self.router.clone();
        let session_for_task = self.session.clone();
        let turn_context_for_task = self.turn_context.clone();
        let sub_id_for_task = sub_id.to_string();

        self.read_only_tasks.spawn(async move {
            let mut tracker = TurnDiffTracker::new();
            let payload_for_fallback = call.payload.clone();
            let call_id_for_fallback = call.call_id.clone();
            let tool_name_for_msg = call.tool_name.clone();
            let fut = async {
                router_for_task
                    .dispatch_tool_call(
                        session_for_task.as_ref(),
                        turn_context_for_task.as_ref(),
                        &mut tracker,
                        &sub_id_for_task,
                        call,
                    )
                    .await
            };

            let response = match AssertUnwindSafe(fut).catch_unwind().await {
                Ok(resp) => resp,
                Err(panic) => {
                    let msg = Self::panic_to_message(panic);
                    let message = format!("{tool_name_for_msg} failed: {msg}");
                    Ok(Self::fallback_response(
                        call_id_for_fallback,
                        payload_for_fallback,
                        message,
                    ))
                }
            };

            (idx, response)
        });
    }

    async fn emit_response_item_events(
        &self,
        sub_id: &str,
        item: &ResponseItem,
    ) -> CodexResult<()> {
        match item {
            ResponseItem::Message { .. }
            | ResponseItem::Reasoning { .. }
            | ResponseItem::WebSearchCall { .. } => {
                let msgs = match item {
                    ResponseItem::Message { .. } if self.turn_context.is_review_mode => {
                        trace!("suppressing assistant Message in review mode");
                        Vec::new()
                    }
                    _ => map_response_item_to_event_messages(
                        item,
                        self.session.show_raw_agent_reasoning(),
                    ),
                };
                for msg in msgs {
                    let event = Event {
                        id: sub_id.to_string(),
                        msg,
                    };
                    self.session.send_event(event).await;
                }
            }
            ResponseItem::FunctionCallOutput { .. } | ResponseItem::CustomToolCallOutput { .. } => {
                debug!("unexpected tool output from stream");
            }
            _ => {}
        }

        Ok(())
    }

    fn fallback_response(
        call_id: String,
        payload: ToolPayload,
        message: String,
    ) -> ResponseInputItem {
        match payload {
            ToolPayload::Custom { .. } => ResponseInputItem::CustomToolCallOutput {
                call_id,
                output: message,
            },
            _ => ResponseInputItem::FunctionCallOutput {
                call_id,
                output: FunctionCallOutputPayload {
                    content: message,
                    success: Some(false),
                },
            },
        }
    }

    fn panic_to_message(payload: Box<dyn std::any::Any + Send>) -> String {
        if let Some(s) = payload.downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = payload.downcast_ref::<String>() {
            s.clone()
        } else {
            "panic without message".to_string()
        }
    }

    fn map_dispatch_error(err: FunctionCallError) -> CodexErr {
        match err {
            FunctionCallError::Fatal(message) => CodexErr::Fatal(message),
            _ => CodexErr::Fatal(err.to_string()),
        }
    }
}
