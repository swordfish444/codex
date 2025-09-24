//! Session-wide mutable state scaffolding.

/// Placeholder for session-persistent state.
#[derive(Debug, Default)]
pub(crate) struct SessionState;

impl SessionState {
    /// Create a new, empty session state.
    pub(crate) fn new() -> Self {
        Self
    }
}
