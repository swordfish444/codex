**DOs**
- **Extract Elicitations:** Move exec/patch approval flows into dedicated modules and call them from the tool runner.
  ```
  // codex_tool_runner.rs
  match event.msg {
      EventMsg::ExecApprovalRequest { command, cwd, .. } => {
          handle_exec_approval_request(
              command, cwd, outgoing.clone(), codex.clone(),
              request_id.clone(), request_id_str.clone(), event.id.clone(),
          ).await;
          continue;
      }
      EventMsg::ApplyPatchApprovalRequest { reason, grant_root, changes } => {
          handle_patch_approval_request(
              reason, grant_root, changes, outgoing.clone(), codex.clone(),
              request_id.clone(), request_id_str.clone(), event.id.clone(),
          ).await;
          continue;
      }
      _ => {}
  }
  ```

- **Propagate Context IDs:** Include `codex_elicitation`, `codex_mcp_tool_call_id`, and `codex_event_id` in elicitation params.
  ```
  #[derive(Serialize)]
  struct PatchApprovalElicitRequestParams {
      message: String,
      #[serde(rename = "requestedSchema")]
      requested_schema: ElicitRequestParamsRequestedSchema,
      codex_elicitation: String,                // "patch-approval"
      codex_mcp_tool_call_id: String,           // request_id_str
      codex_event_id: String,                   // event.id
      #[serde(skip_serializing_if = "Option::is_none")]
      codex_reason: Option<String>,
      #[serde(skip_serializing_if = "Option::is_none")]
      codex_grant_root: Option<PathBuf>,
      codex_changes: HashMap<PathBuf, FileChange>,
  }
  ```

- **Build Safe, Clear Messages:** Use `shlex::try_join` and inline variables with `format!`.
  ```
  let escaped = shlex::try_join(command.iter().map(|s| s.as_str()))
      .unwrap_or_else(|_| command.join(" "));
  let message = format!("Allow Codex to run `{escaped}` in `{}`?",
      cwd.to_string_lossy());
  ```

- **Return JSON-RPC Errors on Param Serialization Failures:** Reuse a shared error code.
  ```
  // codex_tool_runner.rs
  pub(crate) const INVALID_PARAMS_ERROR_CODE: i64 = -32602;

  // exec_approval.rs / patch_approval.rs
  let params_json = serde_json::to_value(&params).map_err(|err| {
      let message = format!("Failed to serialize ...: {err}");
      outgoing.send_error(
          request_id.clone(),
          JSONRPCErrorError { code: INVALID_PARAMS_ERROR_CODE, message, data: None },
      )
  });
  ```

- **Spawn Response Handlers:** Don’t block the agent loop while waiting for elicitation results.
  ```
  let on_response = outgoing.send_request(ElicitRequest::METHOD, Some(params_json)).await;
  let codex = codex.clone();
  let event_id = event_id.clone();
  tokio::spawn(async move { on_patch_approval_response(event_id, on_response, codex).await; });
  ```

- **Deny on Transport/Parse Failures:** Be conservative for both exec and patch approvals.
  ```
  // On oneshot receive failure
  let value = match receiver.await {
      Ok(v) => v,
      Err(err) => {
          error!("request failed: {err:?}");
          let _ = codex.submit(Op::PatchApproval {
              id: event_id.clone(),
              decision: ReviewDecision::Denied,
          }).await;
          return;
      }
  };

  // On JSON parse failure
  let response = serde_json::from_value::<PatchApprovalResponse>(value)
      .unwrap_or_else(|err| {
          error!("failed to deserialize PatchApprovalResponse: {err}");
          PatchApprovalResponse { decision: ReviewDecision::Denied }
      });
  ```

- **Prefer Top-Level Imports:** Import common collections/types instead of writing full paths.
  ```
  use std::collections::HashMap;
  // ...
  pub codex_changes: HashMap<PathBuf, FileChange>,
  ```

- **Pass CWD Explicitly in Tests:** Plumb `cwd` through the tool call instead of touching the current directory.
  ```
  // tests/common/mcp_process.rs
  pub async fn send_codex_tool_call(
      &mut self,
      cwd: Option<PathBuf>,
      prompt: &str,
  ) -> anyhow::Result<i64> {
      let args = CodexToolCallParam { cwd: cwd.map(|p| p.to_string_lossy().into_owned()), /* ... */ };
      // ...
  }

  // Test call-site
  let codex_request_id = mcp_process
      .send_codex_tool_call(Some(cwd.path().to_path_buf()), "please modify the test file")
      .await?;
  ```

- **Keep “mock-model” in Tests:** Avoid provider-specific workarounds; make the tool-call SSE shape compatible with the mock provider.
  ```
  // config.toml
  model = "mock-model"
  approval_policy = "untrusted"
  sandbox_policy = "read-only"
  ```

- **Construct SSE Tool Calls for apply_patch via shell:** Encode the heredoc patch into the shell tool call.
  ```
  // tests/common/responses.rs
  let shell_command = format!("apply_patch <<'EOF'\n{patch}\nEOF");
  let arguments = serde_json::json!({ "command": ["bash", "-lc", shell_command] });
  let sse = json!({
      "choices": [{
          "delta": { "tool_calls": [{ "id": call_id, "function": { "name": "shell", "arguments": arguments } }] },
          "finish_reason": "tool_calls"
      }]
  });
  ```

- **Destructure with `..` in Tests:** Avoid unused-field underscores.
  ```
  let McpHandle { process: mut mcp_process, .. } = create_mcp_process(responses).await?;
  ```

- **Keep Resources Alive with a Handle:** Retain `MockServer` and `TempDir` in a guard struct with a doc comment.
  ```
  /// Keeps the mock server and temp dir alive for the test duration.
  pub struct McpHandle {
      pub process: McpProcess,
      #[allow(dead_code)] server: MockServer,
      #[allow(dead_code)] dir: TempDir,
  }
  ```

- **Create Temp Files Under `TempDir`:** Avoid `NamedTempFile` if a simple path join suffices and write with a trailing newline for stable diffs.
  ```
  let cwd = TempDir::new()?;
  let test_file = cwd.path().join("destination_file.txt");
  std::fs::write(&test_file, "original content\n")?;
  ```

- **Compose Multi-Line Messages Clearly:** Build lines, then `join("\n")`.
  ```
  let mut lines = Vec::new();
  if let Some(r) = &reason { lines.push(r.clone()); }
  lines.push("Allow Codex to apply proposed code changes?".to_string());
  let message = lines.join("\n");
  ```

**DON’Ts**
- **Don’t Inline Collection Paths Repeatedly:**
  ```
  // Bad
  pub codex_changes: std::collections::HashMap<PathBuf, FileChange>;
  ```

- **Don’t Write Test Files to `env::current_dir()`:**
  ```
  // Bad
  let path = env::current_dir()?.join("test_patch_file.txt");
  std::fs::write(&path, "contents")?;
  ```

- **Don’t Change the Test Model to Provider-Specific Variants:**
  ```
  // Bad
  model = "gpt-4.1-mock-model"
  ```

- **Don’t Block Waiting for Elicitation Responses in the Agent Loop:**
  ```
  // Bad
  let value = outgoing.send_request(...).await.await?; // blocks main loop
  ```

- **Don’t Swallow Transport/Parse Errors Without Responding:** Always submit a conservative `Denied`.
  ```
  // Bad
  if receiver.await.is_err() { return; } // leaves Codex waiting forever
  ```

- **Don’t Use Underscore-Prefixed Bindings to Silence Unused Fields:**
  ```
  // Bad
  let McpHandle { process: mut mcp_process, _server, _dir } = create_mcp_process(...).await?;
  ```

- **Don’t Pass Paths as `String` When `PathBuf` Fits Better:**
  ```
  // Bad
  async fn send_codex_tool_call(&mut self, cwd: Option<String>, prompt: &str) -> ...
  ```

- **Don’t Overcomplicate Temp Files in Tests When a Join Works:**
  ```
  // Bad
  let test_file = NamedTempFile::new_in(cwd.path())?;
  ```

- **Don’t Build Messages by Concatenation When `format!` Suffices:**
  ```
  // Bad
  let msg = "Allow Codex to run `".to_string() + &escaped + "` in `" + &cwd + "`?";
  ```