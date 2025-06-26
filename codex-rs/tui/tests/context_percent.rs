use codex_core::{ContentItem, ResponseItem};
use codex_tui::context::{
    approximate_tokens_used, calculate_context_percent_remaining, max_tokens_for_model,
};

#[test]
fn test_approximate_tokens_used_texts() {
    // 4 chars -> 1 token
    let items = vec![ResponseItem::Message {
        role: "user".into(),
        content: vec![ContentItem::InputText {
            text: "abcd".into(),
        }],
    }];
    assert_eq!(approximate_tokens_used(&items), 1);

    // 7 chars -> 2 tokens
    let items = vec![ResponseItem::Message {
        role: "assistant".into(),
        content: vec![ContentItem::OutputText {
            text: "example".into(),
        }],
    }];
    assert_eq!(approximate_tokens_used(&items), 2);
}

#[test]
fn test_approximate_tokens_used_function_calls() {
    // name.len=2 + args.len=7 -> 9 chars -> ceil(9/4)=3
    let items = vec![ResponseItem::FunctionCall {
        name: "fn".into(),
        arguments: "{\"a\":1}".into(),
        call_id: "id".into(),
    }];
    assert_eq!(approximate_tokens_used(&items), 3);
}

#[test]
fn test_max_tokens_for_model_heuristics() {
    assert_eq!(max_tokens_for_model("model-32k"), 32768);
    assert_eq!(max_tokens_for_model("MY-16K-model"), 16384);
    assert_eq!(max_tokens_for_model("foo-8k-bar"), 8192);
    assert_eq!(max_tokens_for_model("unknown-model"), 131072);
}

#[test]
fn test_calculate_context_percent_remaining() {
    // if used=0, remaining=max -> 100%
    let items: Vec<ResponseItem> = vec![];
    let pct = calculate_context_percent_remaining(&items, "foo");
    assert!((pct - 100.0).abs() < 1e-6);

    // used=1 of max=4k -> ~0.0249%
    let items = vec![ResponseItem::Message {
        role: "user".into(),
        content: vec![ContentItem::InputText { text: "a".into() }],
    }];
    let pct = calculate_context_percent_remaining(&items, "4k-model");
    assert!(pct < 100.0 && pct > 99.0);
}
