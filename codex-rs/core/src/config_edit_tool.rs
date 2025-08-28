use crate::codex::Session;
use crate::config::find_codex_home;
use crate::openai_tools::JsonSchema;
use crate::openai_tools::OpenAiTool;
use crate::openai_tools::ResponsesApiTool;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ResponseInputItem;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use crate::protocol::ReviewDecision;
use codex_apply_patch::maybe_parse_apply_patch_verified;
use codex_apply_patch::MaybeApplyPatchVerified;
use codex_apply_patch::ApplyPatchAction;

const CONFIG_TOML_FILE: &str = "config.toml";

// Embed docs at compile time.
const CONFIG_DOCS_MD: &str = include_str!("../../../docs/config.md");

/// get_config() — fetches the current config.toml.
pub(crate) fn create_get_config_tool() -> OpenAiTool {
    OpenAiTool::Function(ResponsesApiTool {
        name: "get_config".to_string(),
        description: "Gets the current ~/.codex/config.toml. If the user asks about their configuration or wants to review it, call this tool and use the result to answer or summarize as needed.".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties: BTreeMap::new(),
            required: Some(vec![]),
            additional_properties: Some(false),
        },
    })
}

/// set_config(new_config: string) — writes the provided TOML to config.toml.
#[derive(Debug, Deserialize)]
struct SetConfigArgs {
    new_config: String,
}

pub(crate) fn create_set_config_tool() -> OpenAiTool {
    let mut properties = BTreeMap::new();
    properties.insert(
        "new_config".to_string(),
        JsonSchema::String {
            description: Some("Full TOML contents to write to ~/.codex/config.toml".to_string()),
        },
    );
    OpenAiTool::Function(ResponsesApiTool {
        name: "set_config".to_string(),
        description: "Overwrites ~/.codex/config.toml with the provided TOML string. If the user requests configuration changes, construct the full desired TOML and call this tool. The value is validated and a diff will be shown for user approval before writing.".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["new_config".to_string()]),
            additional_properties: Some(false),
        },
    })
}

/// show_config_docs() — returns the configuration documentation.
pub(crate) fn create_show_config_docs_tool() -> OpenAiTool {
    OpenAiTool::Function(ResponsesApiTool {
        name: "show_config_docs".to_string(),
        description: "Returns the configuration documentation. If you need to know how to configure something in Codex or what configuration options are available, call this tool and use it to inform your answer.".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties: BTreeMap::new(),
            required: Some(vec![]),
            additional_properties: Some(false),
        },
    })
}

fn resolve_config_path() -> std::io::Result<PathBuf> {
    let mut p = find_codex_home()?;
    p.push(CONFIG_TOML_FILE);
    Ok(p)
}

pub(crate) async fn handle_get_config(
    _session: &Session,
    _arguments: String,
    _sub_id: String,
    call_id: String,
) -> ResponseInputItem {
    let content = match resolve_config_path().and_then(fs::read_to_string) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            return ResponseInputItem::FunctionCallOutput {
                call_id,
                output: FunctionCallOutputPayload {
                    content: format!("failed to read config: {e}"),
                    success: Some(false),
                },
            };
        }
    };
    ResponseInputItem::FunctionCallOutput {
        call_id,
        output: FunctionCallOutputPayload {
            content,
            success: Some(true),
        },
    }
}

pub(crate) async fn handle_set_config(
    session: &Session,
    arguments: String,
    sub_id: String,
    call_id: String,
) -> ResponseInputItem {
    let args: SetConfigArgs = match serde_json::from_str(&arguments) {
        Ok(a) => a,
        Err(e) => {
            return ResponseInputItem::FunctionCallOutput {
                call_id,
                output: FunctionCallOutputPayload {
                    content: format!("failed to parse function arguments: {e}"),
                    success: None,
                },
            }
        }
    };
    // Validate TOML and ensure it can be materialized into a runtime Config.
    let cfg_toml: crate::config::ConfigToml = match toml::from_str(&args.new_config) {
        Ok(v) => v,
        Err(e) => {
            return ResponseInputItem::FunctionCallOutput {
                call_id,
                output: FunctionCallOutputPayload {
                    content: format!("invalid TOML: {e}"),
                    success: Some(false),
                },
            };
        }
    };
    let codex_home = match find_codex_home() {
        Ok(p) => p,
        Err(e) => {
            return ResponseInputItem::FunctionCallOutput {
                call_id,
                output: FunctionCallOutputPayload {
                    content: format!("failed to resolve codex_home: {e}"),
                    success: Some(false),
                },
            };
        }
    };
    if let Err(e) = crate::config::Config::load_from_base_config_with_overrides(
        cfg_toml.clone(),
        crate::config::ConfigOverrides::default(),
        codex_home.clone(),
    ) {
        return ResponseInputItem::FunctionCallOutput {
            call_id,
            output: FunctionCallOutputPayload {
                content: format!("invalid config: {e}"),
                success: Some(false),
            },
        };
    }
    let path = match resolve_config_path() {
        Ok(p) => p,
        Err(e) => {
            return ResponseInputItem::FunctionCallOutput {
                call_id,
                output: FunctionCallOutputPayload {
                    content: format!("failed to resolve config path: {e}"),
                    success: Some(false),
                },
            };
        }
    };
    // Build a synthetic patch showing the proposed change and ask for patch approval.
    let current = match std::fs::read_to_string(&path) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            return ResponseInputItem::FunctionCallOutput {
                call_id,
                output: FunctionCallOutputPayload {
                    content: format!("failed to read existing config: {e}"),
                    success: Some(false),
                },
            };
        }
    };

    let make_lines = |s: &str| {
        let mut v: Vec<&str> = s.split('\n').collect();
        if v.last().is_some_and(|l| l.is_empty()) {
            v.pop();
        }
        v.into_iter().map(|l| l.to_string()).collect::<Vec<String>>()
    };

    let patch_body = if let Some(curr) = &current {
        let mut body = format!("*** Update File: {}\n@@\n", path.display());
        for line in make_lines(curr) {
            body.push_str(&format!("-{}\n", line));
        }
        for line in make_lines(&args.new_config) {
            body.push_str(&format!("+{}\n", line));
        }
        body
    } else {
        let mut body = format!("*** Add File: {}\n", path.display());
        for line in make_lines(&args.new_config) {
            body.push_str(&format!("+{}\n", line));
        }
        body
    };

    let patch_text = format!("*** Begin Patch\n{patch_body}*** End Patch");
    let argv = vec!["apply_patch".to_string(), patch_text];

    let cwd = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("/"));
    let action: ApplyPatchAction = match maybe_parse_apply_patch_verified(&argv, &cwd) {
        MaybeApplyPatchVerified::Body(action) => action,
        MaybeApplyPatchVerified::CorrectnessError(e) => {
            return ResponseInputItem::FunctionCallOutput {
                call_id,
                output: FunctionCallOutputPayload {
                    content: format!("failed to compute patch diff: {e}"),
                    success: Some(false),
                },
            };
        }
        _ => {
            return ResponseInputItem::FunctionCallOutput {
                call_id,
                output: FunctionCallOutputPayload {
                    content: "failed to construct patch diff".to_string(),
                    success: Some(false),
                },
            };
        }
    };

    let rx = session
        .request_patch_approval(
            sub_id.clone(),
            call_id.clone(),
            &action,
            Some("Update Codex configuration file".to_string()),
            None,
        )
        .await;
    match rx.await.unwrap_or_default() {
        ReviewDecision::Approved | ReviewDecision::ApprovedForSession => { /* proceed */ }
        ReviewDecision::Denied | ReviewDecision::Abort => {
            return ResponseInputItem::FunctionCallOutput {
                call_id,
                output: FunctionCallOutputPayload {
                    content: "set_config rejected by user".to_string(),
                    success: None,
                },
            };
        }
    }
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            return ResponseInputItem::FunctionCallOutput {
                call_id,
                output: FunctionCallOutputPayload {
                    content: format!("failed to create config directory: {e}"),
                    success: Some(false),
                },
            };
        }
    }
    match fs::write(&path, args.new_config.as_bytes()) {
        Ok(_) => ResponseInputItem::FunctionCallOutput {
            call_id,
            output: FunctionCallOutputPayload {
                content: format!("wrote {}", path.display()),
                success: Some(true),
            },
        },
        Err(e) => ResponseInputItem::FunctionCallOutput {
            call_id,
            output: FunctionCallOutputPayload {
                content: format!("failed to write config: {e}"),
                success: Some(false),
            },
        },
    }
}

pub(crate) async fn handle_show_config_docs(
    _session: &Session,
    _arguments: String,
    _sub_id: String,
    call_id: String,
) -> ResponseInputItem {
    ResponseInputItem::FunctionCallOutput {
        call_id,
        output: FunctionCallOutputPayload {
            content: CONFIG_DOCS_MD.to_string(),
            success: Some(true),
        },
    }
}
