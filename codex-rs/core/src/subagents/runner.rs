use crate::codex::Codex;
use crate::error::Result as CodexResult;

use super::definition::SubagentDefinition;
use super::registry::SubagentRegistry;

/// Arguments expected for the `subagent.run` tool.
#[derive(serde::Deserialize)]
pub struct RunSubagentArgs {
    pub name: String,
    pub input: String,
    #[serde(default)]
    pub context: Option<String>,
}

/// Run a subagent in a nested Codex session and return the final message.
pub(crate) async fn run(
    sess: &crate::codex::Session,
    turn_context: &crate::codex::TurnContext,
    registry: &SubagentRegistry,
    args: RunSubagentArgs,
    _parent_sub_id: &str,
) -> CodexResult<String> {
    let def: &SubagentDefinition = registry.get(&args.name).ok_or_else(|| {
        crate::error::CodexErr::Stream(format!("unknown subagent: {}", args.name), None)
    })?;

    let mut nested_cfg = (*sess.base_config()).clone();
    nested_cfg.base_instructions = Some(def.instructions.clone());
    nested_cfg.user_instructions = None;
    nested_cfg.approval_policy = turn_context.approval_policy;
    nested_cfg.sandbox_policy = turn_context.sandbox_policy.clone();
    nested_cfg.cwd = turn_context.cwd.clone();
    nested_cfg.include_subagent_tool = false;

    let nested = Codex::spawn(nested_cfg, sess.auth_manager(), None).await?;
    let nested_codex = nested.codex;

    let subagent_id = uuid::Uuid::new_v4().to_string();
    forward_begin(sess, _parent_sub_id, &subagent_id, &def.name).await;

    let text = match args.context {
        Some(ctx) if !ctx.trim().is_empty() => format!("{ctx}\n\n{input}", input = args.input),
        _ => args.input,
    };

    nested_codex
        .submit(crate::protocol::Op::UserInput {
            items: vec![crate::protocol::InputItem::Text { text }],
        })
        .await
        .map_err(|e| {
            crate::error::CodexErr::Stream(format!("failed to submit to subagent: {e}"), None)
        })?;

    let mut last_message: Option<String> = None;
    loop {
        let ev = nested_codex.next_event().await?;
        match ev.msg.clone() {
            crate::protocol::EventMsg::AgentMessage(m) => {
                last_message = Some(m.message);
            }
            crate::protocol::EventMsg::TaskComplete(t) => {
                let _ = nested_codex.submit(crate::protocol::Op::Shutdown).await;
                forward_forwarded(sess, _parent_sub_id, &subagent_id, &def.name, ev.msg).await;
                forward_end(
                    sess,
                    _parent_sub_id,
                    &subagent_id,
                    &def.name,
                    true,
                    t.last_agent_message.clone(),
                )
                .await;
                return Ok(t
                    .last_agent_message
                    .unwrap_or_else(|| last_message.unwrap_or_default()));
            }
            _ => {}
        }
        forward_forwarded(sess, _parent_sub_id, &subagent_id, &def.name, ev.msg).await;
    }
}

async fn forward_begin(
    sess: &crate::codex::Session,
    parent_sub_id: &str,
    subagent_id: &str,
    name: &str,
) {
    sess
        .send_event(crate::protocol::Event {
            id: parent_sub_id.to_string(),
            msg: crate::protocol::EventMsg::SubagentBegin(crate::protocol::SubagentBeginEvent {
                subagent_id: subagent_id.to_string(),
                name: name.to_string(),
            }),
        })
        .await;
}

async fn forward_forwarded(
    sess: &crate::codex::Session,
    parent_sub_id: &str,
    subagent_id: &str,
    name: &str,
    msg: crate::protocol::EventMsg,
) {
    sess
        .send_event(crate::protocol::Event {
            id: parent_sub_id.to_string(),
            msg: crate::protocol::EventMsg::SubagentForwarded(
                crate::protocol::SubagentForwardedEvent {
                    subagent_id: subagent_id.to_string(),
                    name: name.to_string(),
                    event: Box::new(msg),
                },
            ),
        })
        .await;
}

async fn forward_end(
    sess: &crate::codex::Session,
    parent_sub_id: &str,
    subagent_id: &str,
    name: &str,
    success: bool,
    last_agent_message: Option<String>,
) {
    sess
        .send_event(crate::protocol::Event {
            id: parent_sub_id.to_string(),
            msg: crate::protocol::EventMsg::SubagentEnd(crate::protocol::SubagentEndEvent {
                subagent_id: subagent_id.to_string(),
                name: name.to_string(),
                success,
                last_agent_message,
            }),
        })
        .await;
}
