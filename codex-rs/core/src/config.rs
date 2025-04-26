use crate::flags::OPENAI_DEFAULT_MODEL;
use crate::protocol::AskForApproval;
use dirs::home_dir;
use serde::Deserialize;
use std::path::PathBuf;

/// Embedded fallback instructions that mirror the TypeScript CLI’s default
/// system prompt. These are compiled into the binary so a clean install behaves
/// correctly even if the user has not created `~/.codex/instructions.md`.
const EMBEDDED_INSTRUCTIONS: &str = include_str!("../prompt.md");

/// Application configuration loaded from disk and merged with overrides.
#[derive(Default, Deserialize, Debug, Clone)]
pub struct Config {
    /// Optional override of model selection.
    #[serde(default = "default_model")]
    pub model: String,
    /// Default approval policy for executing commands.
    #[serde(default)]
    pub approval_policy: AskForApproval,
    /// System instructions.
    pub instructions: Option<String>,
}

/// Optional overrides for user configuration (e.g., from CLI flags).
#[derive(Default, Debug, Clone)]
pub struct ConfigOverrides {
    pub model: Option<String>,
    pub approval_policy: Option<AskForApproval>,
}

impl Config {
    /// Load configuration, optionally applying overrides (CLI flags). Merges
    /// ~/.codex/config.toml, ~/.codex/instructions.md, embedded defaults, and
    /// any values provided in `overrides` (highest precedence).
    pub fn load_with_overrides(overrides: ConfigOverrides) -> Self {
        let mut cfg: Config = Self::load_from_toml().unwrap_or_default();

        // Instructions: user-provided instructions.md > embedded default.
        cfg.instructions =
            Self::load_instructions().or_else(|| Some(EMBEDDED_INSTRUCTIONS.to_string()));

        // Apply overrides.
        if let Some(model) = overrides.model {
            cfg.model = model;
        }
        if let Some(policy) = overrides.approval_policy {
            cfg.approval_policy = policy;
        }
        cfg
    }

    fn load_from_toml() -> Option<Self> {
        let mut p = codex_dir().ok()?;
        p.push("config.toml");
        let contents = std::fs::read_to_string(&p).ok()?;
        toml::from_str(&contents).ok()
    }

    fn load_instructions() -> Option<String> {
        let mut p = codex_dir().ok()?;
        p.push("instructions.md");
        std::fs::read_to_string(&p).ok()
    }
}

fn default_model() -> String {
    OPENAI_DEFAULT_MODEL.to_string()
}

/// Returns the path to the Codex configuration directory, which is `~/.codex`.
/// Does not verify that the directory exists.
pub fn codex_dir() -> std::io::Result<PathBuf> {
    let mut p = home_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Could not find home directory",
        )
    })?;
    p.push(".codex");
    Ok(p)
}

/// Returns the path to the folder where Codex logs are stored. Does not verify
/// that the directory exists.
pub fn log_dir() -> std::io::Result<PathBuf> {
    let mut p = codex_dir()?;
    p.push("log");
    Ok(p)
}
