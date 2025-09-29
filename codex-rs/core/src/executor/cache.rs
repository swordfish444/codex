use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Clone, Debug, Default)]
/// Thread-safe store of user approvals so repeated commands can reuse
/// previously granted trust.
pub(crate) struct ApprovalCache {
    inner: Arc<Mutex<HashSet<Vec<String>>>>,
}

impl ApprovalCache {
    pub(crate) fn insert(&self, command: Vec<String>) {
        if command.is_empty() {
            return;
        }
        if let Ok(mut guard) = self.inner.lock() {
            guard.insert(command);
        }
    }

    pub(crate) fn snapshot(&self) -> HashSet<Vec<String>> {
        self.inner.lock().map(|g| g.clone()).unwrap_or_default()
    }
}
