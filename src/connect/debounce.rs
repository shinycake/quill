//! A value waiting out a quiet window, and the token its timer carries.
//!
//! The token and the value change together: arming issues a new token for
//! the new value, and a timer fires into the value only while it still
//! holds the token it was started with. Keeping the counter, the token and
//! the value as three driver fields let a place update one and miss
//! another.

/// What waits, if anything.
#[derive(Debug)]
enum Stage<T> {
    Idle,
    Armed { token: u64, value: T },
}

/// One debounce: the latest armed value and the counter that tokens come
/// from. The counter only grows, so a token never comes back.
#[derive(Debug)]
pub(crate) struct Debounce<T> {
    issued: u64,
    stage: Stage<T>,
}

impl<T> Default for Debounce<T> {
    fn default() -> Self {
        Self {
            issued: 0,
            stage: Stage::Idle,
        }
    }
}

impl<T> Debounce<T> {
    /// Wait with `value` under a new token, replacing what waited. Every
    /// earlier timer is stale from here on.
    pub(crate) fn arm(&mut self, value: T) -> u64 {
        self.issued = self.issued.saturating_add(1);
        self.stage = Stage::Armed {
            token: self.issued,
            value,
        };
        self.issued
    }

    /// What waits.
    pub(crate) fn armed(&self) -> Option<&T> {
        match &self.stage {
            Stage::Idle => None,
            Stage::Armed { value, .. } => Some(value),
        }
    }

    /// The token of what waits.
    pub(crate) fn token(&self) -> Option<u64> {
        match &self.stage {
            Stage::Idle => None,
            Stage::Armed { token, .. } => Some(*token),
        }
    }

    pub(crate) fn is_idle(&self) -> bool {
        matches!(self.stage, Stage::Idle)
    }

    /// Take what waits, whatever its token (a flush or a cancel).
    pub(crate) fn take(&mut self) -> Option<T> {
        match std::mem::replace(&mut self.stage, Stage::Idle) {
            Stage::Idle => None,
            Stage::Armed { value, .. } => Some(value),
        }
    }

    /// A timer started under `token` fired: take what waits only when it is
    /// still the latest window. A stale timer takes nothing and leaves the
    /// latest in place.
    pub(crate) fn take_if(&mut self, token: u64) -> Option<T> {
        if self.token() == Some(token) {
            self.take()
        } else {
            None
        }
    }

    /// Drop what waits and stale every timer started so far.
    pub(crate) fn cancel(&mut self) {
        self.stage = Stage::Idle;
        self.issued = self.issued.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::Debounce;

    #[test]
    fn only_the_latest_window_fires() {
        let mut wait: Debounce<&str> = Debounce::default();
        let first = wait.arm("a");
        let second = wait.arm("ab");
        assert_ne!(first, second);
        assert_eq!(wait.take_if(first), None, "stale timer");
        assert_eq!(wait.armed(), Some(&"ab"), "the latest stays");
        assert_eq!(wait.take_if(second), Some("ab"));
        assert!(wait.is_idle());
        assert_eq!(wait.take_if(second), None, "fires once");
    }

    #[test]
    fn cancel_stales_the_timer_and_a_new_arm_gets_a_new_token() {
        let mut wait: Debounce<u8> = Debounce::default();
        let old = wait.arm(1);
        wait.cancel();
        assert!(wait.is_idle() && wait.take_if(old).is_none());
        let fresh = wait.arm(2);
        assert_ne!(fresh, old);
        assert_eq!(wait.take(), Some(2));
    }
}
