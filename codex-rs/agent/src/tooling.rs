use serde::Deserialize;
use serde::Serialize;

/// Represents which apply_patch tool variant a model expects.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ApplyPatchToolType {
    Freeform,
    Function,
}
