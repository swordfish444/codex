use crate::openai_tools::JsonSchema;
use codex_protocol::config_types::ReasoningEffort;
use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct SubagentDefinition {
    pub name: String,
    pub description: String,
    /// Base instructions for this subagent.
    pub instructions: String,
    /// When not set, inherits the parent agent's tool set. When set to an
    /// empty list, no tools are available to the subagent.
    #[serde(default)]
    pub tools: Option<Vec<String>>, // None => inherit; Some(vec) => allow-list

    /// Optional structured output schema. When set, the subagent must return a
    /// single JSON value that validates against this schema. The schema will be
    /// embedded into the subagent's instructions so the model can adhere to it.
    #[serde(default)]
    output_schema: Option<JsonSchema>,

    /// Optional model override for this subagent. When not provided, inherits
    /// the parent session's configured model.
    #[serde(default)]
    pub model: Option<String>,

    /// Optional reasoning effort override for this subagent. When not provided,
    /// inherits the parent session's configured reasoning effort.
    #[serde(default)]
    pub reasoning_effort: Option<ReasoningEffort>,
}

impl SubagentDefinition {
    pub fn from_json_str(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<Self>(s)
    }

    pub fn from_file(path: &Path) -> std::io::Result<Self> {
        let contents = fs::read_to_string(path)?;
        // Surface JSON parsing error with file context
        serde_json::from_str::<Self>(&contents).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid subagent JSON at {}: {e}", path.display()),
            )
        })
    }

    pub(crate) fn output_schema(&self) -> Option<&JsonSchema> {
        self.output_schema.as_ref()
    }

}
