#![deny(unreachable_pub)]

// Public modules that make up the protocol surface.
pub mod config_types;
pub mod mcp_protocol;
pub mod message_history;
pub mod models;
pub mod parse_command;
pub mod plan_tool;
pub mod protocol;

// Convenience prelude that re-exports the full public API of the submodules.
// Prefer importing from `codex_protocol::prelude::*` in downstream crates to
// avoid per-type re-export drift when adding new protocol types.
pub mod prelude {
    pub use crate::config_types::*;
    pub use crate::message_history::*;
    pub use crate::models::*;
    pub use crate::parse_command::*;
    pub use crate::plan_tool::*;
    pub use crate::protocol::*;

    // Keep MCP items under a nested namespace to avoid name collisions with
    // similarly named protocol items (e.g., `InputItem`).
    pub mod mcp {
        pub use crate::mcp_protocol::*;
    }
}
