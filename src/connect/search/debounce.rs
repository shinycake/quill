//! The settle delay of a typed query: idle, or one query waiting for its
//! timer. A new query, a cancel and a commit each move it, and a timer
//! whose `token` is no longer the newest commits nothing.

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TypedDebounce {
    /// Nothing waits. `token` is the newest one handed out.
    Idle { token: u64 },
    /// `query` waits for the timer that carries `token`.
    Pending { token: u64, query: String },
}

impl Default for TypedDebounce {
    fn default() -> Self {
        Self::Idle { token: 0 }
    }
}

impl TypedDebounce {
    fn token(&self) -> u64 {
        match self {
            Self::Idle { token } | Self::Pending { token, .. } => *token,
        }
    }

    /// `query` waits for a new timer; the token to start it with.
    pub(crate) fn arm(&mut self, query: &str) -> u64 {
        let token = self.token().saturating_add(1);
        *self = Self::Pending {
            token,
            query: query.to_string(),
        };
        token
    }

    /// Drop what waits; a timer already started finds a newer token.
    pub(crate) fn cancel(&mut self) {
        *self = Self::Idle {
            token: self.token().saturating_add(1),
        };
    }

    /// The timer of `token` fired: the query it waits for, when it is the
    /// newest timer.
    pub(crate) fn commit(&mut self, token: u64) -> Option<String> {
        match self {
            Self::Pending { token: newest, .. } if *newest == token => {
                match std::mem::replace(self, Self::Idle { token }) {
                    Self::Pending { query, .. } => Some(query),
                    Self::Idle { .. } => None,
                }
            }
            _ => None,
        }
    }

    /// The query that waits for its timer.
    pub(crate) fn pending(&self) -> Option<&str> {
        match self {
            Self::Pending { query, .. } => Some(query),
            Self::Idle { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_newest_timer_commits_once() {
        let mut debounce = TypedDebounce::default();
        let first = debounce.arm("a");
        let second = debounce.arm("ab");
        assert_eq!(debounce.commit(first), None);
        assert_eq!(debounce.pending(), Some("ab"));
        assert_eq!(debounce.commit(second), Some("ab".to_string()));
        assert_eq!(debounce.commit(second), None);
        assert_eq!(debounce.pending(), None);
    }

    #[test]
    fn a_cancel_drops_the_timer_that_was_started() {
        let mut debounce = TypedDebounce::default();
        let token = debounce.arm("a");
        debounce.cancel();
        assert_eq!(debounce.commit(token), None);
        assert!(debounce.arm("b") > token);
    }
}
