//! Configuration object accepted by the `codex` MCP tool-call.
//!
//! This struct is a **thin wrapper** around a subset of the full Codex
//! [`codex_core::config::Config`] surface.  All fields are optional so callers
//! may override only the settings they care about.  During execution the
//! values are translated into a `codex_core::config::ConfigOverrides` instance
//! and merged with the on-disk configuration via
//! `codex_core::config::Config::load_with_overrides()`.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use codex_core::protocol::{AskForApproval, SandboxPermission, SandboxPolicy};

/// Client-supplied configuration for a `codex` tool-call.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct ConfigForToolCall {
    /// Optional override for the model name (e.g. "gpt-4o", "mistral-7b")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Working directory for the session.  If relative, it is resolved against
    /// the server process’ current working directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,

    /// Execution approval policy expressed as the kebab-case variant name
    /// (`unless-allow-listed`, `auto-edit`, `on-failure`, `never`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_policy: Option<String>,

    /// Sandbox permissions using the same string values accepted by the CLI
    /// (e.g. "disk-write-cwd", "network-full-access").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox_permissions: Option<Vec<String>>,

    /// Disable server-side response storage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disable_response_storage: Option<bool>,

    /// Custom system instructions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,

    /// External notifier command.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notify: Option<Vec<String>>,

    /// The *initial user prompt* to start the Codex conversation.
    pub prompt: String,
}

impl ConfigForToolCall {
    /// Convert the caller-supplied overrides into a fully-materialised
    /// [`codex_core::config::Config`].
    pub fn into_config(self) -> std::io::Result<codex_core::config::Config> {
        use AskForApproval::*;

        // --------------------------------------------------------------
        // Map approval-policy string → enum.
        // --------------------------------------------------------------
        let approval_policy_enum = self.approval_policy.and_then(|s| match s.as_str() {
            "unless-allow-listed" => Some(UnlessAllowListed),
            "auto-edit" => Some(AutoEdit),
            "on-failure" => Some(OnFailure),
            "never" => Some(Never),
            _ => None,
        });

        // --------------------------------------------------------------
        // Sandbox permissions → SandboxPolicy.
        // --------------------------------------------------------------
        let sandbox_policy = if let Some(perms) = self.sandbox_permissions {
            let base = std::env::current_dir()?;
            let mut converted = Vec::new();
            for raw in perms {
                match parse_sandbox_permission_with_base_path(&raw, base.clone()) {
                    Ok(p) => converted.push(p),
                    Err(e) => {
                        tracing::warn!("invalid sandbox permission '{raw}': {e}");
                    }
                }
            }
            Some(SandboxPolicy::from(converted))
        } else {
            None
        };

        // Build ConfigOverrides recognised by codex-core.
        let overrides = codex_core::config::ConfigOverrides {
            model: self.model,
            cwd: self.cwd.map(PathBuf::from),
            approval_policy: approval_policy_enum,
            sandbox_policy,
            disable_response_storage: self.disable_response_storage,
        };

        let mut cfg = codex_core::config::Config::load_with_overrides(overrides)?;

        // Apply extra overrides not handled by ConfigOverrides.
        if self.instructions.is_some() {
            cfg.instructions = self.instructions;
        }
        if self.notify.is_some() {
            cfg.notify = self.notify;
        }

        Ok(cfg)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::ConfigForToolCall;
    use schemars::schema_for;
    use serde_json::{json, Value};

    #[test]
    fn codex_tool_call_schema_matches_golden() {
        let schema = schema_for!(ConfigForToolCall);
        let generated: Value = serde_json::to_value(&schema).expect("schema serialises");

        let expected_props: Value = json!({
            "prompt": { "type": "string" },
            "model": { "type": ["string", "null"] },
            "cwd": { "type": ["string", "null"] },
            "approval-policy": { "type": ["string", "null"] },
            "sandbox-permissions": {
                "type": ["array", "null"],
                "items": { "type": "string" }
            },
            "disable-response-storage": { "type": ["boolean", "null"] },
            "instructions": { "type": ["string", "null"] },
            "notify": {
                "type": ["array", "null"],
                "items": { "type": "string" }
            }
        });

        let gen_props = &generated["properties"];

        for (key, expected_val) in expected_props.as_object().unwrap() {
            let got = &gen_props[key];
            assert!(got.is_object(), "property {key} missing from generated schema");

            assert_eq!(got["type"], expected_val["type"], "type mismatch for `{key}`");

            if let Some(items) = expected_val.get("items") {
                assert_eq!(
                    got.get("items").unwrap(),
                    items,
                    "items mismatch for property `{key}`"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Local helpers
// ---------------------------------------------------------------------------

/// Re-implemented copy of `codex_core::approval_mode_cli_arg::parse_sandbox_permission_with_base_path`.
/// The original is `pub(crate)` so not accessible from outside the crate.
fn parse_sandbox_permission_with_base_path(
    raw: &str,
    base_path: PathBuf,
) -> std::io::Result<SandboxPermission> {
    use SandboxPermission::*;

    if let Some(path) = raw.strip_prefix("disk-write-folder=") {
        return if path.is_empty() {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "disk-write-folder=<PATH> requires a non-empty PATH",
            ))
        } else {
            use path_absolutize::*;

            let file = PathBuf::from(path);
            let absolute_path = if file.is_relative() {
                file.absolutize_from(base_path)
            } else {
                file.absolutize()
            }
            .map(|p| p.into_owned())?;

            Ok(DiskWriteFolder { folder: absolute_path })
        };
    }

    match raw {
        "disk-full-read-access" => Ok(DiskFullReadAccess),
        "disk-write-platform-user-temp-folder" => Ok(DiskWritePlatformUserTempFolder),
        "disk-write-platform-global-temp-folder" => Ok(DiskWritePlatformGlobalTempFolder),
        "disk-write-cwd" => Ok(DiskWriteCwd),
        "disk-full-write-access" => Ok(DiskFullWriteAccess),
        "network-full-access" => Ok(NetworkFullAccess),
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("`{raw}` is not a recognised permission"),
        )),
    }
}
