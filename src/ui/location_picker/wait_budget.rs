//! How long a platform lookup may take. The clock stops while the system's
//! permission prompt is open: that wait is the user's, and a fix that comes
//! after they allow it must still land.

use std::time::Duration;

#[derive(Debug, Default)]
pub(super) struct WaitBudget {
    waited: Duration,
}

impl WaitBudget {
    /// `by` more has passed; whether the lookup has now waited `limit`.
    /// Time spent with `prompt_open` does not count.
    pub(super) fn spent(&mut self, by: Duration, prompt_open: bool, limit: Duration) -> bool {
        if !prompt_open {
            self.waited = self.waited.saturating_add(by);
        }
        self.waited >= limit
    }
}

#[cfg(test)]
mod tests {
    use super::WaitBudget;
    use std::time::Duration;

    const LIMIT: Duration = Duration::from_secs(30);
    const TICK: Duration = Duration::from_millis(100);

    #[test]
    fn it_runs_out_after_the_limit() {
        let mut budget = WaitBudget::default();
        for _ in 0..299 {
            assert!(!budget.spent(TICK, false, LIMIT));
        }
        assert!(budget.spent(TICK, false, LIMIT));
    }

    #[test]
    fn an_open_permission_prompt_stops_the_clock() {
        let mut budget = WaitBudget::default();
        for _ in 0..10_000 {
            assert!(!budget.spent(TICK, true, LIMIT), "the user is deciding");
        }
        for _ in 0..299 {
            assert!(!budget.spent(TICK, false, LIMIT));
        }
        assert!(budget.spent(TICK, false, LIMIT), "then the limit applies");
    }
}
