**DOs**
- **Prefix Config As Experimental:** Use `experimental_include_plan_tool` in TOML to gate the plan tool.
```toml
# codex-rs/config.toml
# Enable the experimental plan tool and prompt instructions
experimental_include_plan_tool = true
```

- **Compute Safe Defaults By Model:** Default ON for unknown GPT-like models; OFF otherwise and for known models.
```rust
// codex-rs/core/src/config.rs
let is_unknown_gpt = openai_model_info.is_none() && model.starts_with("gpt-");

Config {
    // ...
    model_supports_reasoning_summaries: cfg.model_supports_reasoning_summaries.unwrap_or(is_unknown_gpt),
    include_plan_tool: include_plan_tool
        .or(cfg.experimental_include_plan_tool)
        .unwrap_or(is_unknown_gpt),
    // ...
}
```

- **Encapsulate Prompt Behavior:** Introduce `InstructionsConfig` and derive extras from the model in one place.
```rust
// codex-rs/core/src/client_common.rs
#[derive(Debug, Default, Clone)]
pub struct InstructionsConfig {
    pub include_plan_tool: bool,
    pub extra_sections: Vec<&'static str>,
}

impl InstructionsConfig {
    pub fn for_model(model: &str, include_plan_tool: bool) -> Self {
        let mut extra_sections = Vec::new();
        if model.starts_with("gpt-4.1") {
            extra_sections.push(APPLY_PATCH_TOOL_INSTRUCTIONS);
        }
        Self { include_plan_tool, extra_sections }
    }
}
```

- **Strip Plan Section When Disabled (Use Markers):** Remove the “Plan updates” section from the prompt if the flag is off.
```rust
// codex-rs/core/src/client_common.rs
impl Prompt {
    pub(crate) fn get_full_instructions(&self, cfg: &InstructionsConfig) -> Cow<'_, str> {
        let mut base = self.base_instructions_override.as_deref().unwrap_or(BASE_INSTRUCTIONS).to_string();

        if !cfg.include_plan_tool {
            let start = "<!-- PLAN_TOOL:START -->";
            let end = "<!-- PLAN_TOOL:END -->";
            if let (Some(s), Some(e)) = (base.find(start), base.find(end)) {
                if e > s {
                    let mut edited = String::with_capacity(base.len());
                    edited.push_str(&base[..s]);
                    edited.push_str(&base[e + end.len()..]);
                    base = edited;
                }
            } else if let Some(idx) = base.find("\n\nPlan updates").or_else(|| base.find("\nPlan updates")).or_else(|| base.find("Plan updates")) {
                base.truncate(idx);
            }
            base = base.trim_end().to_string();
        }

        let mut sections: Vec<&str> = vec![&base];
        for s in &cfg.extra_sections { sections.push(s); }
        Cow::Owned(sections.join("\n"))
    }
}
```
```md
<!-- codex-rs/core/prompt.md -->
<!-- PLAN_TOOL:START -->
Plan updates

A tool named `update_plan` is available...
<!-- PLAN_TOOL:END -->
```

- **Gate Tool Execution And Advertising:** Respect the flag both in the handler and when composing messages.
```rust
// codex-rs/core/src/codex.rs (session field)
include_plan_tool: config.include_plan_tool,

// codex-rs/core/src/codex.rs (tool dispatch)
"update_plan" => {
    if sess.include_plan_tool {
        handle_update_plan(sess, arguments, sub_id, call_id).await
    } else {
        ResponseInputItem::FunctionCallOutput {
            call_id,
            output: FunctionCallOutputPayload {
                content: format!("unsupported call: {name}"),
                success: None,
            },
        }
    }
}
```
```rust
// codex-rs/core/src/client.rs (callers updated)
let instr_cfg = crate::client_common::InstructionsConfig::for_model(
    &self.config.model,
    self.config.include_plan_tool,
);
let full_instructions = prompt.get_full_instructions(&instr_cfg);
```
```rust
// codex-rs/core/src/chat_completions.rs
let instr_cfg = crate::client_common::InstructionsConfig::for_model(model, include_plan_tool);
let full_instructions = prompt.get_full_instructions(&instr_cfg);
```

- **Update Tests For New Behavior:** Add focused tests for defaults and prompt composition.
```rust
// codex-rs/core/src/client_common.rs
#[test]
fn plan_section_removed_when_disabled() {
    let prompt = Prompt::default();
    let cfg = InstructionsConfig::for_model("gpt-4.1", false);
    let full = prompt.get_full_instructions(&cfg);
    assert!(!full.contains("Plan updates"));
    assert!(!full.contains("update_plan"));
    assert!(full.contains(APPLY_PATCH_TOOL_INSTRUCTIONS));
}
```
```rust
// codex-rs/core/src/config.rs
#[test]
fn test_plan_and_reasoning_defaults_known_vs_unknown() -> std::io::Result<()> {
    // unknown GPT-like -> ON
    // unknown non-GPT -> OFF
    // known -> OFF
    // (See full test in PR for details)
    Ok(())
}
```

**DON’Ts**
- **Don’t Expose Plan Tool For Known Models By Default:** Avoid hardcoding it ON globally.
```rust
// Bad: forces ON everywhere
Config { include_plan_tool: true, ..default }
// Good: use model-aware defaults (see DOs)
```

- **Don’t Advertise The Tool When Disabled:** Ensure the prompt omits the “Plan updates” section if not enabled.
```rust
// Bad: unconditional prompt text includes plan tool docs
let full_instructions = BASE_INSTRUCTIONS.to_string();
// Good: strip via markers when disabled (see DOs)
```

- **Don’t Hardcode Enablement In UI Layers:** Let config decide; avoid unconditional `Some(true)` in TUI.
```rust
// Bad: codex-rs/tui/src/lib.rs
include_plan_tool: Some(true),
// Good:
include_plan_tool: None,
```

- **Don’t Pass Raw Flags/Model Strings Around:** Prefer a self-documenting config object.
```rust
// Bad
prompt.get_full_instructions(model, include_plan_tool);
// Good
let cfg = InstructionsConfig::for_model(model, include_plan_tool);
prompt.get_full_instructions(&cfg);
```

- **Don’t Skip Tests After Changing Prompt Composition:** Add or update tests to lock in the new behavior.
```rust
// Bad: no regression tests for prompt filtering
// Good: targeted tests for “plan section removed” and defaults (see DOs)
```