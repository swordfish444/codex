use std::collections::HashMap;
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
use crate::error::Result as CodexResult;
use crate::event_mapping::map_response_item_to_event_messages;
use crate::function_tool::FunctionCallError;
use crate::protocol::Event;
use crate::tools::context::ToolPayload;
use crate::tools::router::Router;
use crate::tools::router::ToolCall;
use crate::tools::spec::ToolSpec;
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
    router: Arc<Router>,
    session: Arc<Session>,
    turn_context: Arc<TurnContext>,
    allow_parallel_read_only: bool,
    read_only_tasks: JoinSet<(usize, Result<ResponseInputItem, String>)>,
    read_only_meta: HashMap<usize, (String, ToolPayload, String)>,
    processed_items: Vec<ProcessedResponseItem>,
}

impl ToolCallExecutor {
    pub(crate) fn new(
        router: Arc<Router>,
        session: Arc<Session>, // todo why
        turn_context: Arc<TurnContext>, // todo why
    ) -> Self {
        let allow_parallel_read_only = router.has_read_only_tools()
            && turn_context.tools_config.enable_parallel_read_only
            && turn_context.client.supports_parallel_read_only_tools();

        Self {
            router,
            session,
            turn_context,
            allow_parallel_read_only,
            read_only_tasks: JoinSet::new(),
            read_only_meta: HashMap::new(),
            processed_items: Vec::new(),
        }
    }

    pub(crate) fn specs(&self) -> &[ToolSpec] {
        self.router.specs()
    }

    pub(crate) fn allow_parallel_read_only(&self) -> bool {
        self.allow_parallel_read_only
    }

    pub(crate) fn drain_ready(&mut self) {
        while let Some(res) = self.read_only_tasks.try_join_next() {
            match res {
                Ok((idx, Ok(response))) => self.assign_parallel_success(idx, response),
                Ok((idx, Err(err))) => self.assign_parallel_failure(idx, err),
                Err(join_err) => {
                    warn!(
                        ?join_err,
                        "parallel read-only task aborted before completion"
                    );
                }
            }
        }
    }

    pub(crate) async fn flush(&mut self) {
        while let Some(res) = self.read_only_tasks.join_next().await {
            match res {
                Ok((idx, Ok(response))) => self.assign_parallel_success(idx, response),
                Ok((idx, Err(err))) => self.assign_parallel_failure(idx, err),
                Err(join_err) => {
                    warn!(
                        ?join_err,
                        "parallel read-only task aborted before completion"
                    );
                }
            }
        }
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
                    self.flush().await;
                    let response = self
                        .router
                        .dispatch_tool_call(
                            self.session.as_ref(),
                            self.turn_context.as_ref(),
                            turn_diff_tracker,
                            sub_id,
                            call,
                        )
                        .await;
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

                self.flush().await;
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
        }

        Ok(())
    }

    pub(crate) fn take_processed_items(mut self) -> Vec<ProcessedResponseItem> {
        self.drain_ready();
        self.processed_items
    }

    fn schedule_parallel_task(&mut self, idx: usize, call: ToolCall, sub_id: &str) {
        let payload_clone = call.payload.clone();
        self.read_only_meta.insert(
            idx,
            (call.call_id.clone(), payload_clone, call.tool_name.clone()),
        );

        let router_for_task = self.router.clone();
        let session_for_task = self.session.clone();
        let turn_context_for_task = self.turn_context.clone();
        let sub_id_for_task = sub_id.to_string();

        self.read_only_tasks.spawn(async move {
            let mut tracker = TurnDiffTracker::new();
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

            let result = AssertUnwindSafe(fut)
                .catch_unwind()
                .await
                .map_err(Self::panic_to_message);

            (idx, result)
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

    fn assign_parallel_success(&mut self, idx: usize, response: ResponseInputItem) {
        self.read_only_meta.remove(&idx);
        if let Some(slot) = self.processed_items.get_mut(idx) {
            slot.response = Some(response);
        } else {
            warn!(idx, "parallel tool completion missing output slot");
        }
    }

    fn assign_parallel_failure(&mut self, idx: usize, reason: String) {
        let (call_id, payload, tool_name) = self.read_only_meta.remove(&idx).unwrap_or_else(|| {
            (
                String::new(),
                ToolPayload::Function {
                    arguments: String::new(),
                },
                String::from("unknown"),
            )
        });

        let message = if tool_name == "unknown" {
            reason
        } else {
            format!("{tool_name} failed: {reason}")
        };

        let response = Self::fallback_response(call_id, payload, message);
        if let Some(slot) = self.processed_items.get_mut(idx) {
            slot.response = Some(response);
        } else {
            warn!(idx, "parallel tool failure missing output slot");
        }
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
}
