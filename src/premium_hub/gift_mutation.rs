//! The one change to the gifts list that may be out at a time (toggle
//! saved, sell, transfer, delete, pin, price, collection changes).
//!
//! It is one value with the request it waits for: only that request's
//! answer ends it, so a late answer of a list the user left and opened
//! again does not unlock the new one, and a lost request fails through the
//! same error path as any other.

/// Where the gifts list's pending change stands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GiftMutation {
    #[default]
    Idle,
    /// The change sent as request `request` is out.
    Running { request: u64 },
}

impl GiftMutation {
    /// A change is out: further ones wait.
    pub fn is_running(self) -> bool {
        matches!(self, Self::Running { .. })
    }

    /// The change sent as `request` goes out.
    pub fn begin(&mut self, request: u64) {
        *self = Self::Running { request };
    }

    /// The change never left: nothing waits.
    pub fn cancel(&mut self) {
        *self = Self::Idle;
    }

    /// `request` answered (ok or error): it ends the change it sent, and
    /// only that one. `false` for another request's answer.
    pub fn settle(&mut self, request: Option<u64>) -> bool {
        match *self {
            Self::Running { request: waiting } if request == Some(waiting) => {
                *self = Self::Idle;
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_request_that_started_it_ends_it() {
        let mut change = GiftMutation::default();
        assert!(!change.is_running());
        change.begin(7);
        assert!(change.is_running());
        assert!(!change.settle(Some(6)), "an older request's answer");
        assert!(!change.settle(None));
        assert!(change.is_running());
        assert!(change.settle(Some(7)));
        assert!(!change.is_running());
        change.begin(8);
        change.cancel();
        assert_eq!(change, GiftMutation::Idle);
    }
}
