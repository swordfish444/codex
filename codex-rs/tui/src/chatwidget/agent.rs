use std::sync::Arc;

use codex_core::CodexConversation;
use codex_core::ConversationManager;
use codex_core::NewConversation;
use codex_core::config::Config;
use codex_core::protocol::Op;
use codex_utils_readiness::Readiness;
use codex_utils_readiness::ReadinessFlag;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::mpsc::unbounded_channel;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;

pub(crate) struct AgentChannels {
    pub(crate) op_tx: UnboundedSender<Op>,
    pub(crate) turn_readiness: UnboundedSender<Arc<ReadinessFlag>>,
}
fn mark_ready(flag: Arc<ReadinessFlag>) {
    tokio::spawn(async move {
        if let Ok(token) = flag.subscribe().await {
            let _ = flag.mark_ready(token).await;
        }
    });
}

fn spawn_readiness_forwarder(
    mut rx: UnboundedReceiver<Arc<ReadinessFlag>>,
    sender: UnboundedSender<Arc<ReadinessFlag>>,
) {
    tokio::spawn(async move {
        while let Some(flag) = rx.recv().await {
            if sender.send(Arc::clone(&flag)).is_err() {
                mark_ready(flag);
            }
        }
    });
}

pub(crate) fn send_turn_readiness(
    sender: &UnboundedSender<Arc<ReadinessFlag>>,
    flag: Arc<ReadinessFlag>,
) {
    if sender.send(Arc::clone(&flag)).is_err() {
        mark_ready(flag);
    }
}

/// Spawn the agent bootstrapper and op forwarding loop, returning the
/// channels used by the UI to submit operations and register turn readiness.
pub(crate) fn spawn_agent(
    config: Config,
    app_event_tx: AppEventSender,
    server: Arc<ConversationManager>,
) -> AgentChannels {
    let (codex_op_tx, mut codex_op_rx) = unbounded_channel::<Op>();
    let (turn_readiness_tx, turn_readiness_rx) = unbounded_channel::<Arc<ReadinessFlag>>();

    let app_event_tx_clone = app_event_tx;
    tokio::spawn(async move {
        let NewConversation {
            conversation_id: _,
            conversation,
            session_configured,
        } = match server.new_conversation(config).await {
            Ok(v) => v,
            Err(e) => {
                // TODO: surface this error to the user.
                tracing::error!("failed to initialize codex: {e}");
                return;
            }
        };

        let readiness_sender = conversation.turn_readiness_sender();
        spawn_readiness_forwarder(turn_readiness_rx, readiness_sender);

        // Forward the captured `SessionConfigured` event so it can be rendered in the UI.
        let ev = codex_core::protocol::Event {
            // The `id` does not matter for rendering, so we can use a fake value.
            id: "".to_string(),
            msg: codex_core::protocol::EventMsg::SessionConfigured(session_configured),
        };
        app_event_tx_clone.send(AppEvent::CodexEvent(ev));

        let conversation_clone = conversation.clone();
        tokio::spawn(async move {
            while let Some(op) = codex_op_rx.recv().await {
                let id = conversation_clone.submit(op).await;
                if let Err(e) = id {
                    tracing::error!("failed to submit op: {e}");
                }
            }
        });

        while let Ok(event) = conversation.next_event().await {
            app_event_tx_clone.send(AppEvent::CodexEvent(event));
        }
    });

    AgentChannels {
        op_tx: codex_op_tx,
        turn_readiness: turn_readiness_tx,
    }
}

/// Spawn agent loops for an existing conversation (e.g., a forked conversation).
/// Sends the provided `SessionConfiguredEvent` immediately, then forwards subsequent
/// events and accepts Ops for submission.
pub(crate) fn spawn_agent_from_existing(
    conversation: std::sync::Arc<CodexConversation>,
    session_configured: codex_core::protocol::SessionConfiguredEvent,
    app_event_tx: AppEventSender,
) -> AgentChannels {
    let (codex_op_tx, mut codex_op_rx) = unbounded_channel::<Op>();
    let (turn_readiness_tx, turn_readiness_rx) = unbounded_channel::<Arc<ReadinessFlag>>();
    spawn_readiness_forwarder(turn_readiness_rx, conversation.turn_readiness_sender());

    let app_event_tx_clone = app_event_tx;
    tokio::spawn(async move {
        // Forward the captured `SessionConfigured` event so it can be rendered in the UI.
        let ev = codex_core::protocol::Event {
            id: "".to_string(),
            msg: codex_core::protocol::EventMsg::SessionConfigured(session_configured),
        };
        app_event_tx_clone.send(AppEvent::CodexEvent(ev));

        let conversation_clone = conversation.clone();
        tokio::spawn(async move {
            while let Some(op) = codex_op_rx.recv().await {
                let id = conversation_clone.submit(op).await;
                if let Err(e) = id {
                    tracing::error!("failed to submit op: {e}");
                }
            }
        });

        while let Ok(event) = conversation.next_event().await {
            app_event_tx_clone.send(AppEvent::CodexEvent(event));
        }
    });

    AgentChannels {
        op_tx: codex_op_tx,
        turn_readiness: turn_readiness_tx,
    }
}
