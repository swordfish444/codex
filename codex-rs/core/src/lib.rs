//! Root of the `codex-core` library.

// Prevent accidental direct writes to stdout/stderr in library code. All
// user-visible output must go through the appropriate abstraction (e.g.,
// the TUI or the tracing stack).
#![deny(clippy::print_stdout, clippy::print_stderr)]

//---------------------------------------------------------------------------
// Test support
//---------------------------------------------------------------------------
// Some helper modules are `#[path]`-included into unit tests that live inside
// `src/` *and* reused by the integration tests under `core/tests`.  They refer
// to the crate as `codex_core::...`. When they are compiled as part of this
// very crate that name would normally not be in scope. The following alias
// makes it available, avoiding conditional compilation tricks in the helpers.

// Alias current crate under the name `codex_core` for the reason explained
// above.
#[cfg(test)]
extern crate self as codex_core;

mod chat_completions;
mod client;
mod client_common;
pub mod codex;
pub use codex::Codex;
pub mod codex_wrapper;
pub mod config;
pub mod config_profile;
mod conversation_history;
pub mod error;
pub mod exec;
pub mod exec_linux;
mod flags;
mod is_safe_command;
#[cfg(target_os = "linux")]
pub mod landlock;
mod mcp_connection_manager;
pub mod mcp_server_config;
mod mcp_tool_call;
mod model_provider_info;
pub use model_provider_info::ModelProviderInfo;
pub use model_provider_info::WireApi;
mod models;
mod project_doc;
pub mod protocol;
mod rollout;
mod safety;
mod user_notification;
pub mod util;
