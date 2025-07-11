#![allow(clippy::unwrap_used, clippy::expect_used)]

use codex_core::exec::{ExecParams, SandboxType, process_exec_tool_call};
use codex_core::protocol::SandboxPolicy;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Notify;

#[tokio::test]
async fn echo_command_outputs_text() {
    let params = ExecParams {
        command: vec!["echo".into(), "Hello".into()],
        cwd: std::env::current_dir().unwrap(),
        timeout_ms: None,
        env: HashMap::new(),
    };
    let policy = SandboxPolicy::new_workspace_write_policy();
    let output = process_exec_tool_call(
        params,
        SandboxType::None,
        Arc::new(Notify::new()),
        &policy,
        &None,
    )
    .await
    .expect("exec failed");
    assert_eq!(output.exit_code, 0);
    assert!(output.stdout.contains("Hello"));
}
