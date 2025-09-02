**DOs**
- Document the new mode: Add `OnRequest` to user docs (e.g., `config.md`) and CLI help with clear sandbox language.
```rust
/// Only ask for approval when the model believes the command needs to run
/// outside the sandbox.
pub enum ApprovalModeCliArg {
    Untrusted,
    OnFailure,
    OnRequest,
    Never,
}
```

- Clarify help strings: Use precise, user-facing copy mentioning “sandbox” and “permissions.”
```rust
/// Only ask for approval when the model believes the command needs to run
/// outside the sandbox.
```

- Encapsulate tool context: Pass an environment/context struct for tool formatting instead of `SandboxPolicy` directly.
```rust
pub struct ToolsContext {
    pub sandbox: Option<SandboxPolicy>,
    pub include_plan_tool: bool,
}

impl Session {
    pub fn tools_context(&self) -> ToolsContext {
        let sandbox = if self.approval_policy == AskForApproval::OnRequest {
            Some(self.sandbox_policy.clone())
        } else {
            None
        };
        ToolsContext { sandbox, include_plan_tool: self.config.include_plan_tool }
    }
}

// Usage
let ctx = sess.tools_context();
let mut stream = sess.client.clone().stream(&prompt, ctx).await?;
```

- Build tools dynamically: Create the shell tool each time based on context; don’t rely on static globals.
```rust
pub fn create_tools_json_for_responses_api(
    prompt: &Prompt,
    model: &str,
    ctx: &ToolsContext,
) -> Result<Vec<serde_json::Value>> {
    let mut tools: Vec<OpenAiTool> = Vec::new();
    tools.push(match ctx.sandbox.clone() {
        Some(sbx) => create_shell_tool_for_sandbox(sbx),
        None => create_default_shell_tool(),
    });
    if ctx.include_plan_tool {
        tools.push(PLAN_TOOL.clone());
    }
    tools.extend(prompt.tools.clone().unwrap_or_default().into_iter().map(mcp_tool_to_openai_tool));
    Ok(tools.into_iter().map(serde_json::to_value).collect::<Result<_, _>>()?)
}
```

- Use json! for test assertions: Prefer a single equality over piecemeal property checks.
```rust
#[test]
fn shell_tool_schema_for_default_model() {
    let tools = create_tools_json_for_responses_api(&Prompt::default(), "gpt-4o-mini", &ToolsContext { sandbox: None, include_plan_tool: true }).unwrap();
    let expected = json!([{
        "name": "shell",
        "description": "Runs a shell command and returns its output.",
        "strict": false,
        "parameters": {
            "type": "object",
            "properties": {
                "command": { "type": "array", "items": { "type": "string" } },
                "workdir": { "type": "string" },
                "timeout": { "type": "number" }
            },
            "required": ["command"],
            "additionalProperties": false
        }
    } /* , plan tool, etc. */]);
    assert_eq!(tools, expected);
}
```

- Skip-serialize optional fields: Avoid noisy wire payloads for unset values.
```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ShellToolCallParams {
    pub command: Vec<String>,
    pub workdir: Option<String>,
    pub timeout_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub with_escalated_permissions: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub justification: Option<String>,
}
```

- Keep `ExecParams` minimal: Only include what execution actually needs; feed escalation flags into safety checks, not the executor.
```rust
pub struct ExecParams {
    pub cwd: PathBuf,
    pub timeout_ms: Option<u64>,
    pub env: HashMap<String, String>,
    pub command: Vec<String>,
}

let safety = assess_command_safety(
    &params.command,
    approval_policy,
    &sandbox_policy,
    &approved_commands,
    request_escalated_privileges, // from tool params
);
```

- Use realistic command vectors in tests: Model how commands are actually passed.
```rust
let command = vec!["git".to_string(), "commit".to_string()];
```

- Make enums testable: Derive `PartialEq` for assertion friendliness.
```rust
#[derive(Debug, PartialEq)]
pub enum SafetyCheck {
    AutoApprove { sandbox_type: SandboxType },
    AskUser,
}
```

- Fix grammar in descriptions: Keep tool text crisp and correct.
```rust
let description = "Runs a shell command and returns its output.".to_string();
```

- Keep tests tidy: Include blank lines between tests for readability.
```rust
#[test]
fn test_a() {
    // ...
}

#[test]
fn test_b() {
    // ...
}
```

**DON’Ts**
- Don’t leak `SandboxPolicy` across layers: Avoid plumbing it through generic APIs; wrap it in a focused context struct.
- Don’t add unused fields to `ExecParams`: If the executor doesn’t use an option (e.g., escalation flags), keep it out.
- Don’t rely on static tool lists: Context affects tool shape; build the list each call.
- Don’t write brittle JSON tests: Avoid manual `as_object()` + `contains_key()` chains; assert full JSON with `json!`.
- Don’t forget docs and help updates: New CLI modes like `OnRequest` need `config.md` and `--help` clarity.
- Don’t use single-string shell commands in tests: Split tokens, e.g., `["git", "commit"]`, not `["git commit"]`.
- Don’t ship awkward copy: Remove unnecessary commas and vague phrasing from tool descriptions and help.
- Don’t serialize null-like fields: Add `#[serde(skip_serializing_if = "Option::is_none")]` for nonstandard optional params.
- Don’t prompt prematurely in `OnRequest`: Let the model request escalation; only ask the user when escalation is actually requested or required.