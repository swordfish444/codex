**PR 2243 Review: Context Messages in Rollouts — DOs and DON’Ts**

**DOs**
- **Persist Context As Messages**: Store user instructions and environment context as conversation messages in rollouts, not request-only payload.
- **Use Helpers**: Create messages via `Prompt::format_user_instructions_message(...)` and `ResponseItem::from(EnvironmentContext::new(...))`.
- **Record In One Call**: Batch messages into a single `sess.record_conversation_items(...)` to avoid extra lock/await.
- **Keep Order Stable**: Append messages in this order: user_instructions, environment_context, then user turn messages.
- **Match Tag Formats**: Use exact wrappers: user_instructions has blank lines; environment_context does not.
- **Adopt New Labels**: Print “Sandbox mode” (not “Sandbox policy”) and include “Network access”.
- **Rely On Display Derives**: Use derived `Display` + kebab-case for `SandboxMode` and `NetworkAccess`.
- **Build Prompt Correctly**: Populate `Prompt { input, store, tools, base_instructions_override }`; omit removed fields.
- **Write Tests For Order/Tags**: Assert role “user”, correct tag start/end, and consistent ordering across requests.
- **Inline Format Captures**: Use Rust 1.58+ captured identifiers in `format!` (e.g., `format!("{var}")`).

```rust
// DO: record both messages in one call (order: UI, then Env).
let ui = sess.get_user_instructions();
let env = EnvironmentContext::new(
    sess.get_cwd().to_path_buf(),
    sess.get_approval_policy(),
    sess.get_sandbox_policy().clone(),
);
let mut boot_items = Vec::new();
if let Some(ui) = ui {
    boot_items.push(Prompt::format_user_instructions_message(&ui));
}
boot_items.push(ResponseItem::from(env));
sess.record_conversation_items(&boot_items).await;

// DO: construct Prompt without removed fields.
let prompt = Prompt {
    input,
    store: !sess.disable_response_storage,
    tools,
    base_instructions_override: sess.base_instructions.clone(),
};

// DO: exact tag formats (UI has blank lines; Env does not).
let ui_msg = Prompt::format_user_instructions_message("be nice");
let cwd = cwd.path().to_string_lossy();
let ap = AskForApproval::OnRequest;
let sm = SandboxMode::ReadOnly;
let na = NetworkAccess::Restricted;
let expected_env_text = format!(
    "<environment_context>\nCurrent working directory: {cwd}\nApproval policy: {ap}\nSandbox mode: {sm}\nNetwork access: {na}\n</environment_context>"
);

// DO: assert order and tags in tests.
assert_message_starts_with(&body["input"][0], "<user_instructions>");
assert_message_ends_with(&body["input"][0], "</user_instructions>");
assert_message_starts_with(&body["input"][1], "<environment_context>");
assert_message_ends_with(&body["input"][1], "</environment_context>");
```

**DON’Ts**
- **Don’t Re-Inject Per Request**: Avoid request-time concatenation of context; rely on stored rollout messages.
- **Don’t Split Recording Calls**: Don’t call `record_conversation_items()` separately for UI and Env.
- **Don’t Use Old Field Names**: Don’t refer to “Sandbox policy”; it’s now “Sandbox mode”.
- **Don’t Change Tag Whitespace**: Don’t add/remove blank lines; UI has blank lines, Env does not.
- **Don’t Reorder Messages**: Don’t put environment_context before user_instructions.
- **Don’t Manually Handcraft Env Text**: Don’t format the env block by hand; use `EnvironmentContext::new` + `ResponseItem::from`.
- **Don’t Add Back Removed Prompt Fields**: Don’t set `Prompt.user_instructions` or `Prompt.environment_context` (they’re gone).
- **Don’t Break Caching Consistency**: Don’t vary tag text, labels, or order across turns; tests depend on stability.

```rust
// DON'T: two separate records (extra lock/await).
// sess.record_conversation_items(&[ui_msg]).await;
// sess.record_conversation_items(&[env_msg]).await;

// DON'T: wrong tag whitespace for Env (extra blank lines).
let bad_env = format!(
    "<environment_context>\n\n...details...\n\n</environment_context>"
);

// DON'T: wrong message order.
let wrong = serde_json::json!([env_msg, ui_msg, user_msg]); // <- breaks tests

// DON'T: use removed fields on Prompt.
let prompt = Prompt {
    input,
    // user_instructions: Some("..."),          // ❌ removed
    // environment_context: Some(env_context),  // ❌ removed
    store: true,
    tools: vec![],
    base_instructions_override: None,
};
```