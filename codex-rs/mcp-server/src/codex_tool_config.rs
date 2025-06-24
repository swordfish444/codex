//! Configuration object accepted by the `codex` MCP tool-call.

use codex_core::protocol::AskForApproval;
use codex_core::protocol::SandboxPolicy;
use mcp_types::Tool;
use mcp_types::ToolInputSchema;
use schemars::JsonSchema;
use schemars::r#gen::SchemaSettings;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::json_to_toml::json_to_toml;

/// Client-supplied configuration for a `codex` tool-call.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct CodexToolCallParam {
    /// The *initial user prompt* to start the Codex conversation.
    pub prompt: String,

    /// Optional override for the model name (e.g. "o3", "o4-mini").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Configuration profile from config.toml to specify default options.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,

    /// Working directory for the session. If relative, it is resolved against
    /// the server process's current working directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,

    /// Execution approval policy expressed as the kebab-case variant name
    /// (`unless-allow-listed`, `auto-edit`, `on-failure`, `never`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_policy: Option<CodexToolCallApprovalPolicy>,

    /// Individual config settings that will override what is in
    /// CODEX_HOME/config.toml.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<HashMap<String, serde_json::Value>>,
}

// Custom enum mirroring `AskForApproval`, but constrained to the subset we
// expose via the tool-call schema.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CodexToolCallApprovalPolicy {
    AutoEdit,
    UnlessAllowListed,
    OnFailure,
    Never,
}

impl From<CodexToolCallApprovalPolicy> for AskForApproval {
    fn from(value: CodexToolCallApprovalPolicy) -> Self {
        match value {
            CodexToolCallApprovalPolicy::AutoEdit => AskForApproval::AutoEdit,
            CodexToolCallApprovalPolicy::UnlessAllowListed => AskForApproval::UnlessAllowListed,
            CodexToolCallApprovalPolicy::OnFailure => AskForApproval::OnFailure,
            CodexToolCallApprovalPolicy::Never => AskForApproval::Never,
        }
    }
}

/// Builds a `Tool` definition (JSON schema etc.) for the Codex tool-call.
pub(crate) fn create_tool_for_codex_tool_call_param() -> Tool {
    let schema = SchemaSettings::draft2019_09()
        .with(|s| {
            s.inline_subschemas = true;
            s.option_add_null_type = false;
        })
        .into_generator()
        .into_root_schema_for::<CodexToolCallParam>();

    #[expect(clippy::expect_used)]
    let schema_value =
        serde_json::to_value(&schema).expect("Codex tool schema should serialise to JSON");

    let tool_input_schema =
        serde_json::from_value::<ToolInputSchema>(schema_value).unwrap_or_else(|e| {
            panic!("failed to create Tool from schema: {e}");
        });

    Tool {
        name: "codex".to_string(),
        input_schema: tool_input_schema,
        description: Some(
            "Run a Codex session. Accepts configuration parameters matching the Codex Config struct.".to_string(),
        ),
        annotations: None,
    }
}

impl CodexToolCallParam {
    /// Returns the initial user prompt to start the Codex conversation and the
    /// effective Config object generated from the supplied parameters.
    pub fn into_config(
        self,
        codex_linux_sandbox_exe: Option<PathBuf>,
    ) -> std::io::Result<(String, codex_core::config::Config)> {
        let Self {
            prompt,
            model,
            profile,
            cwd,
            approval_policy,
            config: cli_overrides,
        } = self;

        // No per-tool-call override for sandbox policy now that the CLI
        // `--sandbox-permission` flag is gone.  We rely on the server-side
        // configuration defaults instead.
        let sandbox_policy: Option<SandboxPolicy> = None;

        // Build the `ConfigOverrides` recognised by codex-core.
        let overrides = codex_core::config::ConfigOverrides {
            model,
            config_profile: profile,
            cwd: cwd.map(PathBuf::from),
            approval_policy: approval_policy.map(Into::into),
            sandbox_policy,
            model_provider: None,
            codex_linux_sandbox_exe,
        };

        let cli_overrides = cli_overrides
            .unwrap_or_default()
            .into_iter()
            .map(|(k, v)| (k, json_to_toml(v)))
            .collect();

        let cfg = codex_core::config::Config::load_with_cli_overrides(cli_overrides, overrides)?;

        Ok((prompt, cfg))
    }
}
