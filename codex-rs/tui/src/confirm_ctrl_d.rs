use std::time::{Duration, Instant};

/// Helper to track and enforce double Ctrl+D confirmation within a timeout.
pub(crate) struct ConfirmCtrlD {
    require_double: bool,
    timeout: Duration,
    deadline: Option<Instant>,
}

impl ConfirmCtrlD {
    /// Create a new ConfirmCtrlD state.
    ///
    /// `require_double` indicates if double Ctrl+D is required to exit.
    /// `timeout_secs` specifies the confirmation window in seconds.
    pub fn new(require_double: bool, timeout_secs: u64) -> Self {
        ConfirmCtrlD {
            require_double,
            timeout: Duration::from_secs(timeout_secs),
            deadline: None,
        }
    }

    /// Handle a Ctrl+D event at the given instant.
    ///
    /// Returns `true` if the event should trigger exit, or `false` to prompt confirmation.
    pub fn handle(&mut self, now: Instant) -> bool {
        if !self.require_double {
            return true;
        }
        if let Some(deadline) = self.deadline {
            if now <= deadline {
                return true;
            }
        }
        // Start or reset confirmation window.
        self.deadline = Some(now + self.timeout);
        false
    }

    /// Clear the confirmation state if the deadline has passed.
    pub fn expire(&mut self, now: Instant) {
        if let Some(deadline) = self.deadline {
            if now > deadline {
                self.deadline = None;
            }
        }
    }

    /// Returns true if a confirmation window is currently active.
    pub fn is_confirming(&self) -> bool {
        self.deadline.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::ConfirmCtrlD;
    use std::time::{Duration, Instant};

    #[test]
    fn exit_without_double_when_disabled() {
        let mut c = ConfirmCtrlD::new(false, 1);
        let now = Instant::now();
        assert!(c.handle(now));
    }

    #[test]
    fn require_double_ctrl_d() {
        let mut c = ConfirmCtrlD::new(true, 2);
        let t0 = Instant::now();
        // First press should not exit
        assert!(!c.handle(t0));
        assert!(c.is_confirming());
        // Before timeout, second press exits
        let t1 = t0 + Duration::from_secs(1);
        assert!(c.handle(t1));
    }

    #[test]
    fn confirmation_expires() {
        let mut c = ConfirmCtrlD::new(true, 1);
        let t0 = Instant::now();
        assert!(!c.handle(t0));
        assert!(c.is_confirming());
        // After timeout, expire() clears state
        let t2 = t0 + Duration::from_secs(2);
        c.expire(t2);
        assert!(!c.is_confirming());
        // Next press should again not exit
        assert!(!c.handle(t2));
    }
}
