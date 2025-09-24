//! Turn-scoped state and active turn metadata scaffolding.

/// Metadata about the currently running turn.
#[derive(Debug, Default)]
pub(crate) struct ActiveTurn;

/// Mutable state for a single turn.
#[derive(Debug, Default)]
pub(crate) struct TurnState;
