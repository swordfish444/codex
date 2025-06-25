use serde_json::{json, Value};
use codex_core::{Prompt, ResponseItem, ContentItem, FunctionCallOutputPayload};

/// Reproduce the `messages` JSON construction from `stream_chat_completions`
fn build_messages(input: Vec<ResponseItem>, model: &str) -> Vec<Value> {
    let mut messages = Vec::new();
    let mut pending = None::<String>;
    let mut buf_user = Vec::new();

    // system instructions
    messages.push(json!({"role": "system", "content": Prompt::default().get_full_instructions(model)}));

    for item in input {
        match item {
            ResponseItem::Message { role, content } if role == "user" && pending.is_some() => {
                let mut text = String::new();
                for c in content {
                    if let ContentItem::InputText { text: t } = c {
                        text.push_str(&t);
                    }
                }
                buf_user.push(json!({"role": "user", "content": text}));
            }
            ResponseItem::Message { role, content } => {
                let mut text = String::new();
                for c in content {
                    if let ContentItem::InputText { text: t } = c {
                        text.push_str(&t);
                    }
                }
                messages.push(json!({"role": role, "content": text}));
            }
            ResponseItem::FunctionCall { name, arguments, call_id } => {
                pending = Some(call_id.clone());
                messages.push(json!({
                    "role": "assistant", "content": null,
                    "tool_calls": [{"id": call_id, "type": "function", "function": {"name": name, "arguments": arguments}}]
                }));
            }
            ResponseItem::FunctionCallOutput { call_id, output } => {
                messages.push(json!({"role": "tool", "tool_call_id": call_id, "content": output.content}));
                if pending.as_ref() == Some(&call_id) {
                    pending = None;
                    for m in buf_user.drain(..) {
                        messages.push(m);
                    }
                }
            }
            _ => {}
        }
    }

    // cancellation: no output arrived
    if let Some(call_id) = pending {
        messages.push(json!({"role": "tool", "tool_call_id": call_id, "content": "Tool cancelled"}));
        for m in buf_user.drain(..) {
            messages.push(m);
        }
    }

    messages
}

#[test]
fn normal_flow_no_buffer() {
    let input = vec![ResponseItem::Message { role: "user".into(), content: vec![ContentItem::InputText { text: "hi".into() }] }];
    let msgs = build_messages(input, "m1");
    assert_eq!(msgs.iter().filter(|m| m["role"] == json!("user")).count(), 1);
}

#[test]
fn buffer_and_flush_on_output() {
    let call_id = "c1".to_string();
    let input = vec![
        ResponseItem::FunctionCall { name: "f".into(), arguments: "{}".into(), call_id: call_id.clone() },
        ResponseItem::Message { role: "user".into(), content: vec![ContentItem::InputText { text: "late".into() }] },
        ResponseItem::FunctionCallOutput { call_id: call_id.clone(), output: FunctionCallOutputPayload { content: "ok".into(), success: None } },
    ];
    let msgs = build_messages(input, "m1");
    // order: system, functioncall, tool output, then buffered user
    let roles: Vec<_> = msgs.iter().map(|m| m["role"].clone()).collect();
    assert_eq!(roles.as_slice(), &[json!("system"), json!("assistant"), json!("tool"), json!("user")]);
}

#[test]
fn buffer_and_cancel() {
    let call_id = "c2".to_string();
    let input = vec![
        ResponseItem::FunctionCall { name: "f".into(), arguments: "{}".into(), call_id: call_id.clone() },
        ResponseItem::Message { role: "user".into(), content: vec![ContentItem::InputText { text: "oops".into() }] },
    ];
    let msgs = build_messages(input, "m1");
    // expect system, functioncall, fake cancel, then user
    let roles: Vec<_> = msgs.iter().map(|m| m["role"].clone()).collect();
    assert_eq!(roles.as_slice(), &[json!("system"), json!("assistant"), json!("tool"), json!("user")]);
    // cancellation message content
    assert_eq!(msgs[2]["content"], json!("Tool cancelled"));
}
