//! Session/turn state module.
//!
//! Encapsulates all mutable state for a Codex session and the currently active
//! turn. The goal is to present lock-safe, narrow APIs so other modules never
//! need to poke at raw mutexes or internal fields.
//!
//! Locking guidelines
//! - Lock ordering: `SessionState` → `ActiveTurn` → `TurnState`.
//! - Never hold a lock across an `.await`. Extract minimal data and drop the
//!   guard before awaiting.
//! - Prefer helper methods on these types rather than exposing fields.

mod session;
mod turn;

pub(crate) use session::SessionState;
pub(crate) use turn::ActiveTurn;
pub(crate) use turn::TurnState;
