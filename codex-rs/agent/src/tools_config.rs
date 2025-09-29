use crate::model_family::ModelFamily;
use crate::tooling::ApplyPatchToolType;

#[derive(Debug, Clone)]
pub enum ConfigShellToolType {
    Default,
    Local,
    Streamable,
}

#[derive(Debug, Clone)]
pub struct ToolsConfig {
    pub shell_type: ConfigShellToolType,
    pub plan_tool: bool,
    pub apply_patch_tool_type: Option<ApplyPatchToolType>,
    pub web_search_request: bool,
    pub include_view_image_tool: bool,
    pub experimental_unified_exec_tool: bool,
}

pub struct ToolsConfigParams<'a> {
    pub model_family: &'a ModelFamily,
    pub include_plan_tool: bool,
    pub include_apply_patch_tool: bool,
    pub include_web_search_request: bool,
    pub use_streamable_shell_tool: bool,
    pub include_view_image_tool: bool,
    pub experimental_unified_exec_tool: bool,
}

impl ToolsConfig {
    pub fn new(params: &ToolsConfigParams) -> Self {
        let ToolsConfigParams {
            model_family,
            include_plan_tool,
            include_apply_patch_tool,
            include_web_search_request,
            use_streamable_shell_tool,
            include_view_image_tool,
            experimental_unified_exec_tool,
        } = params;
        let shell_type = if *use_streamable_shell_tool {
            ConfigShellToolType::Streamable
        } else if model_family.uses_local_shell_tool {
            ConfigShellToolType::Local
        } else {
            ConfigShellToolType::Default
        };

        let apply_patch_tool_type = match model_family.apply_patch_tool_type {
            Some(ApplyPatchToolType::Freeform) => Some(ApplyPatchToolType::Freeform),
            Some(ApplyPatchToolType::Function) => Some(ApplyPatchToolType::Function),
            None => {
                if *include_apply_patch_tool {
                    Some(ApplyPatchToolType::Freeform)
                } else {
                    None
                }
            }
        };

        Self {
            shell_type,
            plan_tool: *include_plan_tool,
            apply_patch_tool_type,
            web_search_request: *include_web_search_request,
            include_view_image_tool: *include_view_image_tool,
            experimental_unified_exec_tool: *experimental_unified_exec_tool,
        }
    }
}
