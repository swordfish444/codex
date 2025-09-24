//! Turn-scoped state and active turn metadata scaffolding.

use std::sync::Arc;

/// Metadata about the currently running turn.
#[derive(Default)]
pub(crate) struct ActiveTurn {
    pub(crate) sub_id: String,
    pub(crate) turn_state: Arc<TurnState>,
}

/// Mutable state for a single turn.
#[derive(Default)]
pub(crate) struct TurnState;
