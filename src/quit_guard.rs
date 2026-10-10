//! "Show warning before quitting with Cmd+Q" (macOS).
//!
//! Telegram Desktop copies Chromium's confirm-to-quit experiment
//! (`platform/mac/specific_mac.mm` `PreventsQuit`, `ConfirmQuit::RunModal`):
//! a quit that comes from the keyboard shows "Hold Cmd+Q to Quit" and only
//! goes through once the key has been held for a moment. GPUI delivers a
//! key-repeat as a fresh action, so a hold shows up here as a chain of
//! presses with short gaps; this type tells the first press, the rest of the
//! chain and the press that completes the hold apart.

/// How long Cmd+Q must be held (Chromium's `kShowDuration` is 1.5 s).
pub const HOLD_MS: u64 = 1_500;

/// Longest pause between the first press and the first key repeat (macOS
/// "Delay until repeat" goes up to two seconds).
pub const REPEAT_DELAY_MS: u64 = 2_100;

/// Longest pause between two key repeats; a slower rhythm is tapping, not
/// holding.
pub const REPEAT_GAP_MS: u64 = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuitDecision {
    /// Quit now.
    Quit,
    /// First press of a hold: show "Hold Cmd+Q to Quit".
    Warn,
    /// Still holding, not long enough yet.
    Holding,
}

#[derive(Debug, Default)]
pub struct QuitGuard {
    /// When the current chain started, when it was last extended (ms) and
    /// how many presses it holds.
    chain: Option<(u64, u64, u32)>,
}

impl QuitGuard {
    /// Register a quit key press at `now_ms`. With the warning off, every
    /// press quits.
    pub fn press(&mut self, now_ms: u64, warn_enabled: bool) -> QuitDecision {
        if !warn_enabled {
            return QuitDecision::Quit;
        }
        match self.chain {
            Some((start, last, count))
                if now_ms.saturating_sub(last)
                    <= if count == 1 {
                        REPEAT_DELAY_MS
                    } else {
                        REPEAT_GAP_MS
                    } =>
            {
                // Two taps are not a hold: it takes the first press plus at
                // least two repeats spread over the hold time.
                if count >= 2 && now_ms.saturating_sub(start) >= HOLD_MS {
                    self.chain = None;
                    QuitDecision::Quit
                } else {
                    self.chain = Some((start, now_ms, count + 1));
                    QuitDecision::Holding
                }
            }
            _ => {
                self.chain = Some((now_ms, now_ms, 1));
                QuitDecision::Warn
            }
        }
    }
}

/// Toast text, with the platform's quit chord spelled out.
pub const WARNING: &str = "Hold \u{2318}Q to quit";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warning_off_quits_at_once() {
        let mut guard = QuitGuard::default();
        assert_eq!(guard.press(0, false), QuitDecision::Quit);
    }

    #[test]
    fn a_single_tap_only_warns() {
        let mut guard = QuitGuard::default();
        assert_eq!(guard.press(0, true), QuitDecision::Warn);
        // Nothing follows: the chain goes stale and a later tap warns again.
        assert_eq!(guard.press(10_000, true), QuitDecision::Warn);
    }

    #[test]
    fn holding_the_key_quits_after_the_hold_time() {
        let mut guard = QuitGuard::default();
        assert_eq!(guard.press(0, true), QuitDecision::Warn);
        // Key repeat after the initial delay, then every ~100 ms.
        assert_eq!(guard.press(500, true), QuitDecision::Holding);
        let mut decision = QuitDecision::Holding;
        let mut t = 500;
        while decision == QuitDecision::Holding {
            t += 100;
            decision = guard.press(t, true);
        }
        assert_eq!(decision, QuitDecision::Quit);
        assert_eq!(t, HOLD_MS);
    }

    #[test]
    fn slow_taps_do_not_quit() {
        let mut guard = QuitGuard::default();
        assert_eq!(guard.press(0, true), QuitDecision::Warn);
        assert_eq!(guard.press(1_800, true), QuitDecision::Holding);
        // Tapping at that pace is no key repeat: the chain restarts.
        assert_eq!(guard.press(3_600, true), QuitDecision::Warn);
    }

    #[test]
    fn a_pause_restarts_the_hold() {
        let mut guard = QuitGuard::default();
        assert_eq!(guard.press(0, true), QuitDecision::Warn);
        assert_eq!(guard.press(600, true), QuitDecision::Holding);
        assert_eq!(guard.press(700, true), QuitDecision::Holding);
        // Released for longer than the chain gap, then pressed again.
        assert_eq!(
            guard.press(700 + REPEAT_GAP_MS + 1, true),
            QuitDecision::Warn
        );
    }
}
