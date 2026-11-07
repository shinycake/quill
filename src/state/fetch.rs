//! One value for a fetched resource: not asked, in flight, answered, or
//! failed. It replaces a `loading: bool` and an `error` stored beside the
//! data, which could be set in combinations that mean nothing (loading and
//! failed at once, an error with no request).
//!
//! The previous answer stays reachable while a refetch runs or after it
//! fails, so a list does not blank out under a spinner or an error line.

/// The state of a fetched `T`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fetch<T> {
    /// Never asked.
    Idle,
    /// A request is in flight; `stale` is the last answer, if any.
    Loading { stale: Option<T> },
    /// The latest request answered.
    Ready(T),
    /// The latest request failed; `stale` is the last answer, if any.
    Failed { error: String, stale: Option<T> },
}

impl<T> Default for Fetch<T> {
    fn default() -> Self {
        Self::Idle
    }
}

impl<T> Fetch<T> {
    pub fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    /// The error line of a failed request.
    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed { error, .. } => Some(error),
            _ => None,
        }
    }

    /// The newest answer held: the current one, or the one a refetch or a
    /// failure left in place.
    pub fn data(&self) -> Option<&T> {
        match self {
            Self::Idle => None,
            Self::Ready(value) => Some(value),
            Self::Loading { stale } | Self::Failed { stale, .. } => stale.as_ref(),
        }
    }

    pub fn data_mut(&mut self) -> Option<&mut T> {
        match self {
            Self::Idle => None,
            Self::Ready(value) => Some(value),
            Self::Loading { stale } | Self::Failed { stale, .. } => stale.as_mut(),
        }
    }

    /// The held data, moving the state to `Idle`.
    pub fn take_data(&mut self) -> Option<T> {
        match std::mem::take(self) {
            Self::Idle => None,
            Self::Ready(value) => Some(value),
            Self::Loading { stale } | Self::Failed { stale, .. } => stale,
        }
    }

    /// Create the data with `make` when there is none. The state otherwise
    /// stays as it is: an optimistic local edit does not end a request in
    /// flight.
    pub fn insert_data_if_missing(&mut self, make: impl FnOnce() -> T) {
        if self.data().is_some() {
            return;
        }
        *self = match std::mem::take(self) {
            Self::Loading { .. } => Self::Loading {
                stale: Some(make()),
            },
            Self::Failed { error, .. } => Self::Failed {
                error,
                stale: Some(make()),
            },
            Self::Idle | Self::Ready(_) => Self::Ready(make()),
        };
    }

    /// A request goes out. The last answer stays visible, the last error is
    /// gone.
    pub fn begin(&mut self) {
        let stale = self.take_data();
        *self = Self::Loading { stale };
    }

    /// The request answered.
    pub fn succeed(&mut self, value: T) {
        *self = Self::Ready(value);
    }

    /// The request failed; the last answer stays visible.
    pub fn fail(&mut self, error: impl Into<String>) {
        let stale = self.take_data();
        *self = Self::Failed {
            error: error.into(),
            stale,
        };
    }

    /// The request failed and what was held is wrong now.
    pub fn fail_dropping_data(&mut self, error: impl Into<String>) {
        *self = Self::Failed {
            error: error.into(),
            stale: None,
        };
    }

    /// The request never left (a send error that is not a result): back to
    /// what was held before it.
    pub fn cancel(&mut self) {
        if self.is_loading() {
            *self = match self.take_data() {
                Some(value) => Self::Ready(value),
                None => Self::Idle,
            };
        }
    }

    /// Forget the last error, keeping the data. A request in flight or an
    /// answer is left alone.
    pub fn clear_error(&mut self) {
        if matches!(self, Self::Failed { .. }) {
            *self = match self.take_data() {
                Some(value) => Self::Ready(value),
                None => Self::Idle,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Fetch;

    #[test]
    fn a_fresh_fetch_is_idle_with_nothing() {
        let fetch: Fetch<Vec<i32>> = Fetch::default();
        assert!(fetch.is_idle());
        assert!(!fetch.is_loading());
        assert_eq!(fetch.data(), None);
        assert_eq!(fetch.error(), None);
    }

    #[test]
    fn loading_keeps_the_last_answer_and_drops_the_error() {
        let mut fetch = Fetch::Failed {
            error: "no network".to_string(),
            stale: Some(vec![1]),
        };
        fetch.begin();
        assert!(fetch.is_loading());
        assert_eq!(fetch.data(), Some(&vec![1]));
        assert_eq!(fetch.error(), None);
        fetch.succeed(vec![2]);
        assert_eq!(fetch, Fetch::Ready(vec![2]));
    }

    #[test]
    fn failing_keeps_the_last_answer() {
        let mut fetch = Fetch::Ready(vec![1]);
        fetch.begin();
        fetch.fail("timeout");
        assert_eq!(fetch.error(), Some("timeout"));
        assert_eq!(fetch.data(), Some(&vec![1]));
        assert!(!fetch.is_loading());
    }

    #[test]
    fn failing_can_drop_the_data() {
        let mut fetch = Fetch::Ready(vec![1]);
        fetch.fail_dropping_data("bad");
        assert_eq!(fetch.error(), Some("bad"));
        assert_eq!(fetch.data(), None);
    }

    #[test]
    fn cancel_returns_to_what_was_held() {
        let mut empty: Fetch<i32> = Fetch::Idle;
        empty.begin();
        empty.cancel();
        assert!(empty.is_idle());
        let mut held = Fetch::Ready(5);
        held.begin();
        held.cancel();
        assert_eq!(held, Fetch::Ready(5));
        // Not loading: nothing to cancel.
        let mut failed: Fetch<i32> = Fetch::Failed {
            error: "x".into(),
            stale: None,
        };
        failed.cancel();
        assert_eq!(failed.error(), Some("x"));
    }

    #[test]
    fn clearing_the_error_keeps_data_and_in_flight_requests() {
        let mut failed = Fetch::Failed {
            error: "x".to_string(),
            stale: Some(3),
        };
        failed.clear_error();
        assert_eq!(failed, Fetch::Ready(3));
        let mut bare: Fetch<i32> = Fetch::Failed {
            error: "x".into(),
            stale: None,
        };
        bare.clear_error();
        assert!(bare.is_idle());
        let mut loading: Fetch<i32> = Fetch::Loading { stale: None };
        loading.clear_error();
        assert!(loading.is_loading());
    }

    #[test]
    fn inserting_data_keeps_the_request_state() {
        let mut idle: Fetch<Vec<i32>> = Fetch::Idle;
        idle.insert_data_if_missing(Vec::new);
        idle.data_mut().into_iter().for_each(|list| list.push(1));
        assert_eq!(idle, Fetch::Ready(vec![1]));

        let mut loading: Fetch<Vec<i32>> = Fetch::Loading { stale: None };
        loading.insert_data_if_missing(Vec::new);
        loading.data_mut().into_iter().for_each(|list| list.push(2));
        assert_eq!(
            loading,
            Fetch::Loading {
                stale: Some(vec![2])
            }
        );

        let mut failed: Fetch<Vec<i32>> = Fetch::Failed {
            error: "e".into(),
            stale: None,
        };
        failed.insert_data_if_missing(Vec::new);
        failed.data_mut().into_iter().for_each(|list| list.push(3));
        assert_eq!(failed.error(), Some("e"));
        assert_eq!(failed.data(), Some(&vec![3]));

        let mut ready = Fetch::Ready(vec![4]);
        ready.insert_data_if_missing(Vec::new);
        ready.data_mut().into_iter().for_each(|list| list.push(5));
        assert_eq!(ready, Fetch::Ready(vec![4, 5]));
    }

    #[test]
    fn take_data_leaves_idle() {
        let mut fetch = Fetch::Ready(7);
        assert_eq!(fetch.take_data(), Some(7));
        assert!(fetch.is_idle());
    }
}
