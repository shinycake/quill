//! Push-to-talk for group calls: the pure state machine.
//!
//! tdesktop (`calls/group/calls_group_call.cpp`, `pushToTalk` +
//! `Core::App().settings().groupCallPushToTalk()` / `pushToTalkDelay`):
//! with push-to-talk on, the microphone stays muted until the shortcut is
//! held; releasing it mutes again after the configured delay, and pressing
//! again inside that delay keeps the microphone open.
//!
//! Quill listens to the key in its own windows only (no global hook on any
//! platform yet), so the state machine is fed key events by the UI and told
//! the time explicitly; it never reads a clock.

use serde::{Deserialize, Serialize};

/// Default key: Space, like most voice apps.
pub const DEFAULT_PTT_KEY: &str = "space";
/// Selectable release delays in milliseconds (`pushToTalkDelay` in tdesktop
/// is a free slider from 20 to 1000 ms; presets keep the UI a radio group).
pub const RELEASE_DELAYS_MS: [u32; 4] = [20, 200, 500, 1000];
/// Upper bound enforced on loaded values.
pub const MAX_RELEASE_DELAY_MS: u32 = 1000;

/// Persisted push-to-talk settings (part of `CallPrefs`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PttConfig {
    pub enabled: bool,
    /// GPUI key name (`Keystroke::key`, lower case), e.g. `space`, `f13`, `t`.
    pub key: String,
    pub release_delay_ms: u32,
}

impl Default for PttConfig {
    fn default() -> Self {
        PttConfig {
            enabled: false,
            key: DEFAULT_PTT_KEY.to_string(),
            release_delay_ms: 200,
        }
    }
}

impl PttConfig {
    /// A usable copy: empty keys fall back to the default, delays are capped.
    pub fn sanitized(&self) -> PttConfig {
        let key = self.key.trim().to_ascii_lowercase();
        PttConfig {
            enabled: self.enabled,
            key: if key.is_empty() {
                DEFAULT_PTT_KEY.to_string()
            } else {
                key
            },
            release_delay_ms: self.release_delay_ms.min(MAX_RELEASE_DELAY_MS),
        }
    }
}

/// What the call should do with the microphone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PttAction {
    /// Open the microphone (key pressed).
    Talk,
    /// Close the microphone again (key released and the delay elapsed).
    Mute,
}

/// Key-driven talk state with a release delay.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PushToTalk {
    held: bool,
    /// Deadline (ms on the caller's clock) at which a released key mutes.
    mute_at: Option<u64>,
    talking: bool,
}

impl PushToTalk {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the microphone is currently open because of push-to-talk.
    pub fn is_talking(&self) -> bool {
        self.talking
    }

    /// Pending mute deadline, so the UI can schedule one wake-up.
    pub fn mute_deadline(&self) -> Option<u64> {
        self.mute_at
    }

    /// Whether `key` is the configured push-to-talk key.
    pub fn matches(config: &PttConfig, key: &str) -> bool {
        config.enabled && config.sanitized().key == key.to_ascii_lowercase()
    }

    /// The key went down. Auto-repeat while held is ignored.
    pub fn key_down(&mut self) -> Option<PttAction> {
        if self.held {
            return None;
        }
        self.held = true;
        self.mute_at = None;
        if self.talking {
            // Pressed again inside the release delay: stay open.
            None
        } else {
            self.talking = true;
            Some(PttAction::Talk)
        }
    }

    /// The key came up at `now_ms`. With no delay the mute is immediate.
    pub fn key_up(&mut self, now_ms: u64, delay_ms: u32) -> Option<PttAction> {
        if !self.held {
            return None;
        }
        self.held = false;
        if !self.talking {
            return None;
        }
        if delay_ms == 0 {
            self.talking = false;
            return Some(PttAction::Mute);
        }
        self.mute_at = Some(now_ms.saturating_add(u64::from(delay_ms)));
        None
    }

    /// Advance the clock; fires the delayed mute once its deadline passed.
    pub fn tick(&mut self, now_ms: u64) -> Option<PttAction> {
        match self.mute_at {
            Some(deadline) if now_ms >= deadline && !self.held => {
                self.mute_at = None;
                self.talking = false;
                Some(PttAction::Mute)
            }
            _ => None,
        }
    }

    /// Window lost focus, the call ended or PTT was switched off: drop any
    /// held/pending state. Returns `Mute` when the microphone was open.
    pub fn reset(&mut self) -> Option<PttAction> {
        let was_talking = self.talking;
        *self = PushToTalk::default();
        was_talking.then_some(PttAction::Mute)
    }
}

#[cfg(test)]
mod tests {
    use super::{PttAction, PttConfig, PushToTalk};

    #[test]
    fn press_talks_and_release_mutes_after_the_delay() {
        let mut ptt = PushToTalk::new();
        assert_eq!(ptt.key_down(), Some(PttAction::Talk));
        assert!(ptt.is_talking());
        assert_eq!(ptt.key_up(1_000, 200), None);
        assert!(ptt.is_talking(), "still open during the release delay");
        assert_eq!(ptt.mute_deadline(), Some(1_200));
        assert_eq!(ptt.tick(1_199), None);
        assert_eq!(ptt.tick(1_200), Some(PttAction::Mute));
        assert!(!ptt.is_talking());
        assert_eq!(ptt.tick(5_000), None, "mutes once");
    }

    #[test]
    fn zero_delay_mutes_on_release() {
        let mut ptt = PushToTalk::new();
        ptt.key_down();
        assert_eq!(ptt.key_up(10, 0), Some(PttAction::Mute));
        assert!(!ptt.is_talking());
    }

    #[test]
    fn auto_repeat_is_ignored() {
        let mut ptt = PushToTalk::new();
        assert_eq!(ptt.key_down(), Some(PttAction::Talk));
        assert_eq!(ptt.key_down(), None);
        assert_eq!(ptt.key_down(), None);
    }

    #[test]
    fn pressing_inside_the_delay_cancels_the_mute() {
        let mut ptt = PushToTalk::new();
        ptt.key_down();
        ptt.key_up(0, 500);
        assert_eq!(ptt.key_down(), None, "already open, no second unmute");
        assert_eq!(ptt.mute_deadline(), None);
        assert_eq!(ptt.tick(10_000), None, "held key never mutes");
        assert_eq!(ptt.key_up(10_000, 500), None);
        assert_eq!(ptt.tick(10_500), Some(PttAction::Mute));
    }

    #[test]
    fn stray_release_does_nothing() {
        let mut ptt = PushToTalk::new();
        assert_eq!(ptt.key_up(0, 200), None);
        assert!(!ptt.is_talking());
    }

    #[test]
    fn reset_mutes_an_open_microphone_once() {
        let mut ptt = PushToTalk::new();
        ptt.key_down();
        assert_eq!(ptt.reset(), Some(PttAction::Mute));
        assert_eq!(ptt.reset(), None);
        assert_eq!(ptt.key_down(), Some(PttAction::Talk), "usable again");
    }

    #[test]
    fn config_matching_and_sanitizing() {
        let mut cfg = PttConfig::default();
        assert!(!PushToTalk::matches(&cfg, "space"), "off by default");
        cfg.enabled = true;
        assert!(PushToTalk::matches(&cfg, "space"));
        assert!(PushToTalk::matches(&cfg, "SPACE"));
        assert!(!PushToTalk::matches(&cfg, "t"));
        cfg.key = String::new();
        cfg.release_delay_ms = 99_999;
        let clean = cfg.sanitized();
        assert_eq!(clean.key, "space");
        assert_eq!(clean.release_delay_ms, 1000);
    }

    #[test]
    fn old_prefs_without_ptt_fields_load() {
        let cfg: PttConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(cfg, PttConfig::default());
    }
}
