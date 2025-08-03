//! Small shared helpers for slash-command argument parsing and execution-mode display.

use codex_core::protocol::AskForApproval;
use codex_core::protocol::SandboxPolicy;

/// Canonical execution presets that combine approval policy and sandbox policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionPreset {
    /// never prompt; read-only FS
    ReadOnly,
    /// ask to retry outside sandbox only on sandbox breach; read-only FS
    Untrusted,
    /// auto within workspace sandbox; ask to retry outside on breach
    Auto,
    /// DANGEROUS: disables sandbox and approvals entirely.
    FullYolo,
}

impl ExecutionPreset {
    pub fn label(self) -> &'static str {
        match self {
            ExecutionPreset::ReadOnly => "Read only",
            ExecutionPreset::Untrusted => "Untrusted",
            ExecutionPreset::Auto => "Auto",
            ExecutionPreset::FullYolo => "Full yolo",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            ExecutionPreset::ReadOnly => {
                "never prompt; read-only filesystem (flags: --ask-for-approval never --sandbox read-only)"
            }
            ExecutionPreset::Untrusted => {
                "ask to retry outside sandbox only on sandbox breach; read-only (flags: --ask-for-approval on-failure --sandbox read-only)"
            }
            ExecutionPreset::Auto => {
                "auto in workspace sandbox; ask to retry outside sandbox on breach (flags: --ask-for-approval on-failure --sandbox workspace-write)"
            }
            ExecutionPreset::FullYolo => {
                "DANGEROUS: disables sandbox and approvals; the agent can run any commands with full system access (flags: --dangerously-bypass-approvals-and-sandbox)"
            }
        }
    }

    /// Canonical CLI flags string for the preset.
    #[allow(dead_code)]
    pub fn cli_flags(self) -> &'static str {
        match self {
            ExecutionPreset::ReadOnly => "--ask-for-approval never --sandbox read-only",
            ExecutionPreset::Untrusted => "--ask-for-approval on-failure --sandbox read-only",
            ExecutionPreset::Auto => "--ask-for-approval on-failure --sandbox workspace-write",
            ExecutionPreset::FullYolo => "--dangerously-bypass-approvals-and-sandbox",
        }
    }

    /// Mapping from preset to policies.
    pub fn to_policies(self) -> (AskForApproval, SandboxPolicy) {
        match self {
            ExecutionPreset::ReadOnly => (AskForApproval::Never, SandboxPolicy::ReadOnly),
            ExecutionPreset::Untrusted => (AskForApproval::OnFailure, SandboxPolicy::ReadOnly),
            ExecutionPreset::Auto => (
                AskForApproval::OnFailure,
                SandboxPolicy::WorkspaceWrite {
                    writable_roots: vec![],
                    network_access: false,
                    include_default_writable_roots: true,
                },
            ),
            ExecutionPreset::FullYolo => (AskForApproval::Never, SandboxPolicy::DangerFullAccess),
        }
    }

    /// Mapping from policies to a known preset.
    pub fn from_policies(
        approval: AskForApproval,
        sandbox: &SandboxPolicy,
    ) -> Option<ExecutionPreset> {
        match (approval, sandbox) {
            (AskForApproval::Never, SandboxPolicy::ReadOnly) => Some(ExecutionPreset::ReadOnly),
            (AskForApproval::OnFailure, SandboxPolicy::ReadOnly) => {
                Some(ExecutionPreset::Untrusted)
            }
            (AskForApproval::OnFailure, SandboxPolicy::WorkspaceWrite { .. }) => {
                Some(ExecutionPreset::Auto)
            }
            (AskForApproval::Never, SandboxPolicy::DangerFullAccess)
            | (AskForApproval::OnFailure, SandboxPolicy::DangerFullAccess) => {
                Some(ExecutionPreset::FullYolo)
            }
            _ => None,
        }
    }

    /// Parse one of the canonical tokens: read-only | untrusted | auto.
    pub fn parse_token(s: &str) -> Option<ExecutionPreset> {
        let t = s.trim().to_ascii_lowercase();
        let t = t.replace(' ', "-");
        match t.as_str() {
            "read-only" => Some(ExecutionPreset::ReadOnly),
            "untrusted" => Some(ExecutionPreset::Untrusted),
            "auto" => Some(ExecutionPreset::Auto),
            "full-yolo" => Some(ExecutionPreset::FullYolo),
            _ => None,
        }
    }
}

/// Strip a single pair of surrounding quotes from the provided string if present.
/// Supports straight and common curly quotes: '…', "…", ‘…’, “…”.
pub fn strip_surrounding_quotes(s: &str) -> &str {
    // Opening/closing pairs (note curly quotes differ on each side)
    const QUOTE_PAIRS: &[(char, char)] = &[('\"', '\"'), ('\'', '\''), ('“', '”'), ('‘', '’')];

    let t = s.trim();
    if t.len() < 2 {
        return t;
    }

    for &(open, close) in QUOTE_PAIRS {
        if t.starts_with(open) && t.ends_with(close) {
            let start = open.len_utf8();
            let end = t.len() - close.len_utf8();
            return &t[start..end];
        }
    }

    t
}

/// Normalize a free-form token: trim whitespace and remove a single pair of surrounding quotes.
pub fn normalize_token(s: &str) -> String {
    strip_surrounding_quotes(s).trim().to_string()
}

/// Map an (approval, sandbox) pair to a concise preset label used in the UI.
pub fn execution_mode_label(approval: AskForApproval, sandbox: &SandboxPolicy) -> &'static str {
    ExecutionPreset::from_policies(approval, sandbox)
        .map(|p| p.label())
        .unwrap_or("Custom")
}

/// Describe the current execution preset including CLI flag equivalents.
#[allow(dead_code)]
pub fn execution_mode_description(
    approval: AskForApproval,
    sandbox: &SandboxPolicy,
) -> &'static str {
    ExecutionPreset::from_policies(approval, sandbox)
        .map(|p| p.description())
        .unwrap_or("custom combination")
}

/// Parse a free-form token to an execution preset (approval+sandbox).
pub fn parse_execution_mode_token(s: &str) -> Option<(AskForApproval, SandboxPolicy)> {
    ExecutionPreset::parse_token(s).map(|p| p.to_policies())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_quotes_variants() {
        assert_eq!(strip_surrounding_quotes("\"o3\""), "o3");
        assert_eq!(strip_surrounding_quotes("'o3'"), "o3");
        assert_eq!(strip_surrounding_quotes("“o3”"), "o3");
        assert_eq!(strip_surrounding_quotes("‘o3’"), "o3");
        assert_eq!(strip_surrounding_quotes("o3"), "o3");
        assert_eq!(strip_surrounding_quotes("  o3  "), "o3");
    }

    #[test]
    fn parse_execution_mode_aliases() {
        use codex_core::protocol::AskForApproval;
        use codex_core::protocol::SandboxPolicy;
        let parse = parse_execution_mode_token;
        assert!(matches!(
            parse("auto"),
            Some((
                AskForApproval::OnFailure,
                SandboxPolicy::WorkspaceWrite { .. }
            ))
        ));
        assert_eq!(
            parse("untrusted"),
            Some((AskForApproval::OnFailure, SandboxPolicy::ReadOnly))
        );
        assert_eq!(
            parse("read-only"),
            Some((AskForApproval::Never, SandboxPolicy::ReadOnly))
        );
        assert_eq!(
            parse("full-yolo"),
            Some((AskForApproval::Never, SandboxPolicy::DangerFullAccess))
        );
        assert_eq!(
            parse("Full Yolo"),
            Some((AskForApproval::Never, SandboxPolicy::DangerFullAccess))
        );
        assert_eq!(parse("unknown"), None);
        assert!(parse("  AUTO  ").is_some());
    }
}
