**DOs**

- Boldly update MCP schema version: set to 2025-06-18 and regenerate types.
  ```rust
  // codex-rs/mcp-types/src/lib.rs
  pub const MCP_SCHEMA_VERSION: &str = "2025-06-18";
  ```
  ```python
  # codex-rs/mcp-types/generate_mcp_types.py
  SCHEMA_VERSION = "2025-06-18"
  ```
  ```md
  # codex-rs/mcp-types/README.md
  https://modelcontextprotocol.io/specification/2025-06-18/basic
  ```

- Prefer ContentBlock everywhere tool results or prompt content appear.
  ```rust
  use mcp_types::{CallToolResult, ContentBlock, TextContent};

  let result = CallToolResult {
      content: vec![ContentBlock::TextContent(TextContent {
          r#type: "text".into(),
          text: "Done".into(),
          annotations: None,
          _meta: None,
      })],
      is_error: None,
      structured_content: None,
  };
  ```

- Return structuredContent for machine-readable tool output.
  ```rust
  use serde_json::json;

  let result = CallToolResult {
      content: vec![ContentBlock::TextContent(TextContent {
          r#type: "text".into(),
          text: "Indexed 3 files".into(),
          annotations: None,
          _meta: None,
      })],
      is_error: None,
      structured_content: Some(json!({ "indexed_count": 3 })),
  };
  ```

- Handle new ResourceLink in UIs and clients.
  ```rust
  use mcp_types::ContentBlock;

  match block {
      ContentBlock::TextContent(t) => println!("{}", t.text),
      ContentBlock::ImageContent(_) => println!("<image>"),
      ContentBlock::AudioContent(_) => println!("<audio>"),
      ContentBlock::EmbeddedResource(r) => println!("embedded: {}", r.resource_uri()),
      ContentBlock::ResourceLink(link) => println!("link: {}", link.uri),
  }
  ```

- Add title to Implementation and Tool for better UX.
  ```rust
  use mcp_types::Implementation;

  let client_info = Implementation {
      name: "codex-mcp-client".into(),
      title: Some("Codex".into()),
      version: env!("CARGO_PKG_VERSION").into(),
  };
  ```
  ```rust
  use mcp_types::{Tool, ToolInputSchema};

  let tool = Tool {
      name: "codex".into(),
      title: Some("Codex".into()),
      description: Some("Run a Codex session.".into()),
      input_schema: ToolInputSchema { r#type: "object".into(), properties: None, required: None },
      output_schema: None,
      annotations: None,
      _meta: None,
  };
  ```

- Implement elicitation support: advertise capability and handle requests/results.
  ```rust
  use mcp_types::{ClientCapabilities, ElicitRequestParams, ElicitResult, ElicitRequestParamsRequestedSchema};

  let capabilities = ClientCapabilities {
      elicitation: Some(serde_json::json!({})),
      experimental: None, roots: None, sampling: None
  };

  let req = ElicitRequestParams {
      message: "Please confirm deletion".into(),
      requested_schema: ElicitRequestParamsRequestedSchema {
          r#type: "object".into(),
          properties: serde_json::json!({
              "confirm": { "type": "boolean", "title": "Confirm" }
          }),
          required: Some(vec!["confirm".into()]),
      },
  };

  let res = ElicitResult { action: "accept".into(), content: Some(serde_json::json!({"confirm": true})) };
  ```

- Use ResourceTemplateReference when completing arguments (replaces ResourceReference).
  ```rust
  use mcp_types::{CompleteRequestParamsRef, ResourceTemplateReference};

  let r = CompleteRequestParamsRef::ResourceTemplateReference(ResourceTemplateReference {
      r#type: "ref/resource".into(),
      uri: "file:///repo/{path}".into(),
  });
  ```

- Replace JSON-RPC batch handling with single-message handling.
  ```rust
  use mcp_types::JSONRPCMessage;

  match msg {
      JSONRPCMessage::Request(r) => process_request(r),
      JSONRPCMessage::Notification(n) => process_notification(n),
      JSONRPCMessage::Response(r) => process_response(r),
      JSONRPCMessage::Error(e) => process_error(e),
  }
  ```

- Box error payloads where needed and deref at callsites.
  ```rust
  // callee
  fn parse_container_exec_arguments(...) -> Result<ExecParams, Box<ResponseInputItem>> {
      // ...
      Err(Box::new(output))
  }

  // caller
  let params = match parse_container_exec_arguments(arguments, sess, &call_id) {
      Ok(p) => p,
      Err(output) => return *output,
  };
  ```

- Map enum property safely with r#enum when generating/using schemas.
  ```rust
  use mcp_types::EnumSchema;

  let schema = EnumSchema {
      r#enum: vec!["one".into(), "two".into()],
      enum_names: None,
      description: Some("Choose a value".into()),
      title: Some("Value".into()),
      r#type: "string".into(),
  };
  ```

- Update tests to include new fields and versions.
  ```rust
  use mcp_types::{InitializeRequestParams, Implementation, MCP_SCHEMA_VERSION};

  let params = InitializeRequestParams {
      capabilities: Default::default(),
      client_info: Implementation { name: "acme-client".into(), title: Some("Acme".into()), version: "1.2.3".into() },
      protocol_version: MCP_SCHEMA_VERSION.into(),
  };
  ```

- Keep tool error reporting inside CallToolResult with is_error = true.
  ```rust
  let err = CallToolResult {
      content: vec![ContentBlock::TextContent(TextContent {
          r#type: "text".into(),
          text: format!("Failed to start Codex session: {e}"),
          annotations: None,
          _meta: None,
      })],
      is_error: Some(true),
      structured_content: None,
  };
  ```

**DON’Ts**

- Don’t use removed types: CallToolResultContent, PromptMessageContent, ResourceReference, JSONRPCBatchRequest/JSONRPCBatchResponse.
  ```rust
  // BAD (removed):
  // let x: CallToolResultContent = CallToolResultContent::TextContent(...);
  // let y: JSONRPCMessage = JSONRPCMessage::BatchRequest(vec![]);
  ```

- Don’t forget structuredContent when returning machine-readable results.
  ```rust
  // BAD: drops structure, harder for clients to consume
  CallToolResult { content: vec![...], is_error: None, structured_content: None }
  ```

- Don’t ignore ResourceLink in renderers; links will be lost to users.
  ```rust
  // BAD: missing match arm for ContentBlock::ResourceLink(...)
  ```

- Don’t send protocol-level JSONRPCError for tool-level failures; set is_error instead.
  ```rust
  // BAD:
  // JSONRPCMessage::Error(JSONRPCError { ... "tool failed" ... })
  ```

- Don’t omit title in Implementation/Tool when a human-friendly label exists.
  ```rust
  // BAD:
  Implementation { name: "codex-mcp-client".into(), title: None, version: "…".into() }
  ```

- Don’t return unboxed error values when the function signature expects Box.
  ```rust
  // BAD:
  // fn parse(... ) -> Result<_, Box<ResponseInputItem>> { Err(output) } // not boxed
  ```

- Don’t keep JSON-RPC batch handling code; it’s no longer part of the schema.
  ```rust
  // BAD:
  // fn process_batch_request(...) { … }
  // fn process_batch_response(...) { … }
  ```

- Don’t leave stray typos or debug text in comments.
  ```diff
  - // Run the Codex session and stream events Fck to the client.
  + // Run the Codex session and stream events to the client.
  ```