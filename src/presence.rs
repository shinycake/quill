//! Batch 4: when the account shows as online. tdesktop
//! (`Api::Updates::updateOnline`) is online while its window is active
//! and the user was not idle for `offlineIdleTimeout`; a window in the
//! background, hidden or minimised is offline. TDLib's `online` option
//! defaults to false and re-announces itself while true, so only changes
//! are sent.

/// tdesktop `MTP::Config::offlineIdleTimeout` default (30 s): no input
/// for this long and the account goes offline even with the window open.
pub const OFFLINE_IDLE_MS: u64 = 30_000;

/// Online while the window is active and the user is not idle.
pub fn should_be_online(window_active: bool, idle_ms: u64) -> bool {
    window_active && idle_ms < OFFLINE_IDLE_MS
}

/// Milliseconds until an online account turns idle, for scheduling a
/// re-check; `None` when it is not online.
pub fn idle_in_ms(window_active: bool, idle_ms: u64) -> Option<u64> {
    should_be_online(window_active, idle_ms).then(|| OFFLINE_IDLE_MS - idle_ms)
}

/// Remembers the last value sent so `online` is only set on change.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PresenceSync {
    sent: Option<bool>,
}

impl PresenceSync {
    /// The value to send now, if it differs from the last one sent.
    pub fn next(&mut self, desired: bool) -> Option<bool> {
        if self.sent == Some(desired) {
            return None;
        }
        self.sent = Some(desired);
        Some(desired)
    }

    /// A send failed or the connection restarted (TDLib forgets the
    /// option across sessions): the next `next` sends again.
    pub fn reset(&mut self) {
        self.sent = None;
    }

    pub fn last_sent(&self) -> Option<bool> {
        self.sent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn online_only_while_active_and_not_idle() {
        assert!(should_be_online(true, 0));
        assert!(should_be_online(true, OFFLINE_IDLE_MS - 1));
        assert!(!should_be_online(true, OFFLINE_IDLE_MS));
        assert!(!should_be_online(false, 0));
    }

    #[test]
    fn idle_countdown_only_while_online() {
        assert_eq!(idle_in_ms(true, 10_000), Some(20_000));
        assert_eq!(idle_in_ms(true, 40_000), None);
        assert_eq!(idle_in_ms(false, 0), None);
    }

    #[test]
    fn sync_sends_changes_once() {
        let mut sync = PresenceSync::default();
        assert_eq!(sync.next(false), Some(false), "first value always goes out");
        assert_eq!(sync.next(false), None);
        assert_eq!(sync.next(true), Some(true));
        assert_eq!(sync.next(true), None);
        assert_eq!(sync.next(false), Some(false));
        sync.reset();
        assert_eq!(sync.next(false), Some(false), "re-sent after a restart");
    }
}
