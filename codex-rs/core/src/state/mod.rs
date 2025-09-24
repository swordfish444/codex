//! Session/turn state module scaffolding.
//!
//! This module will encapsulate all mutable state for a Codex session.
//! It starts with lightweight placeholders to enable incremental refactors
//! without changing behaviour.

mod session;
mod turn;

pub(crate) use session::SessionState;
