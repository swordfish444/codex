**DOs**
- Add docstrings with links: document new types and variants that model custom tools, and reference the Responses API docs.
- Handle both tool styles: support Freeform (“custom tools”) and Function (“JSON tools”) everywhere tools are parsed, generated, logged, recorded, and persisted.
- Log with inline variables: include tool names and key fields directly in log messages for clarity.
- Model capability via enums: use `Option<ApplyPatchToolType>` to encode whether and how a model prefers `apply_patch`.
- Select tools centrally: decide which `apply_patch` tool to expose in one place (`get_openai_tools`) based on `apply_patch_tool_type`.
- Keep prompt/tool presence in sync: when checking for an existing `apply_patch` tool, look for both Function and Freeform variants before injecting instructions.
- Consume new SSE events: recognize and ignore streaming deltas for custom tool inputs, only acting on completed items.
- Record round-trips: add both `CustomToolCall` and `CustomToolCallOutput` to conversation history and rollout logs.
- Map outputs correctly: convert container exec outputs back into `CustomToolCallOutput` when responding to custom tools.
- Prefer self-documenting construction: consider a typed arg struct (or builder) for `ToolsConfig::new` calls to clarify parameters.
- Make tests portable or explicit: either ensure tests run on Windows or clearly annotate/skip them with a reason.

Code examples:

```rust
/// Custom tool invocation emitted by models that support “custom tools”.
/// Carries raw `input` and expects raw string output (no JSON schema).
/// See: https://platform.openai.com/docs/guides/function-calling#custom-tools
pub enum ResponseItem {
    // ...
    CustomToolCall {
        call_id: String,
        name: String,
        input: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<String>,
    },
    /// Output corresponding to a `CustomToolCall`.
    CustomToolCallOutput {
        call_id: String,
        output: String,
    },
    // ...
}
```

```rust
/// Describes a custom (freeform) tool for the Responses API.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FreeformTool {
    pub name: String,
    pub description: String,
    pub format: FreeformToolFormat, // grammar type, syntax, definition
}
```

```rust
// Model capability signaling (prefer declarative enum over booleans)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ApplyPatchToolType {
    Freeform,  // “custom tool”
    Function,  // JSON function tool
}

pub struct ModelFamily {
    // ...
    pub apply_patch_tool_type: Option<ApplyPatchToolType>,
}
```

```rust
// Centralized tool selection
pub(crate) fn get_openai_tools(cfg: &ToolsConfig, _mcp: Option<HashMap<_, _>>) -> Vec<OpenAiTool> {
    let mut tools = vec![create_shell(cfg.shell_type)];
    if cfg.plan_tool { tools.push(PLAN_TOOL.clone()); }

    if let Some(kind) = &cfg.apply_patch_tool_type {
        match kind {
            ApplyPatchToolType::Freeform => tools.push(create_apply_patch_freeform_tool()),
            ApplyPatchToolType::Function => tools.push(create_apply_patch_json_tool()),
        }
    }
    tools
}
```

```rust
// Presence check for instructions injection
let has_apply_patch = tools.iter().any(|t| match t {
    OpenAiTool::Function(f) => f.name == "apply_patch",
    OpenAiTool::Freeform(f) => f.name == "apply_patch",
    _ => false,
});
```

```rust
// SSE: consume custom tool input deltas; act on final items
match event_type.as_str() {
    "response.custom_tool_call_input.delta"
    | "response.custom_tool_call_input.done"
    | "response.in_progress"
    | "response.output_item.added"
    | "response.output_text.done" => {
        // absorb/ignore; stateful assembly if needed
    }
    _ => { /* existing handling */ }
}
```

```rust
// Record both call and output in history/rollout
if let (ResponseItem::CustomToolCall { .. },
        Some(ResponseInputItem::CustomToolCallOutput { call_id, output })) = (&item, next_input) {
    items_to_record_in_conversation_history.push(item.clone());
    items_to_record_in_conversation_history.push(
        ResponseItem::CustomToolCallOutput { call_id: call_id.clone(), output: output.clone() }
    );
}
```

```rust
// Map container exec reply back to custom tool output
match resp {
    ResponseInputItem::FunctionCallOutput { call_id, output } => {
        ResponseInputItem::CustomToolCallOutput { call_id, output: output.content }
    }
    other => other,
}
```

```rust
// Inline variable logging (concise and searchable)
info!("FunctionCall: {name}({arguments})");
info!("CustomToolCall: {name} {input}");
```

```rust
// Self-documenting constructor pattern for ToolsConfig
pub struct ToolsConfigInit {
    pub shell_type: ConfigShellToolType,
    pub plan_tool: bool,
    pub include_apply_patch_tool: bool,
    pub model_family: ModelFamily,
}

impl ToolsConfig {
    pub fn new(init: ToolsConfigInit) -> Self {
        let apply_patch_tool_type = init.model_family.apply_patch_tool_type
            .or(if init.include_apply_patch_tool { Some(ApplyPatchToolType::Freeform) } else { None });
        Self { shell_type: init.shell_type, plan_tool: init.plan_tool, apply_patch_tool_type }
    }
}
```

```rust
// Portable test annotations: skip with reason on Windows if required
#[cfg_attr(target_os = "windows", ignore = "Requires POSIX shell and path semantics")]
#[tokio::test]
async fn test_apply_patch_freeform_tool() -> anyhow::Result<()> {
    // ...
    Ok(())
}
```

**DON’Ts**
- Don’t ship new public types or enum variants without docstrings and a brief link to the relevant API docs.
- Don’t assume all models support custom tools; always fall back to the JSON tool when `apply_patch_tool_type` requests `Function` or when unspecified in non-default contexts.
- Don’t forget to update every pipeline stage (SSE client, handler, conversation history, rollout, abort paths) to include `CustomToolCall` and `CustomToolCallOutput`.
- Don’t log ambiguous messages; include `name`, `arguments`, or `input` inline to aid debugging.
- Don’t pass long positional arg lists to constructors like `ToolsConfig::new`; prefer a struct/builder to avoid parameter order mistakes.
- Don’t introduce platform-only tests without gating or documenting why they’re skipped on that platform.
- Don’t emit apply_patch instructions when an `apply_patch` tool (Function or Freeform) is already present.