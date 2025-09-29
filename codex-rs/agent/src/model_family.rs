use crate::config_types::ReasoningSummaryFormat;
use crate::tooling::ApplyPatchToolType;

/// Metadata describing consistent behaviour across a family of models.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModelFamily {
    pub slug: String,
    pub family: String,
    pub needs_special_apply_patch_instructions: bool,
    pub supports_reasoning_summaries: bool,
    pub reasoning_summary_format: ReasoningSummaryFormat,
    pub uses_local_shell_tool: bool,
    pub apply_patch_tool_type: Option<ApplyPatchToolType>,
    pub base_instructions: String,
}
