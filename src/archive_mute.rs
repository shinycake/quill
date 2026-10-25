//! "Archive and mute new chats from non-contacts": when the setting is
//! offered, and when to suggest it (pure, no UI).
//!
//! Telegram Desktop shows the switch in Privacy and Security and in the
//! archive settings box only while the server allows it, the setting is
//! already on, or the account has Premium (`SetupArchiveAndMute`,
//! `GlobalPrivacy::showArchiveAndMute`); once shown it stays. When the
//! server suggests it (TDLib `suggestedActionEnableArchiveAndMuteNewChats`)
//! and the setting is off, tdesktop asks "Hide new chats?" with a button to
//! the settings (`SessionController::suggestArchiveAndMute`); closing the
//! box dismisses the suggestion.
#![deny(clippy::indexing_slicing)]

use crate::telegram::envelope::OptionValue;
use crate::telegram::requests::ArchiveChatListSettings;
use std::collections::BTreeSet;

/// The suggested action behind "Hide new chats?".
pub const ACTION: &str = "suggestedActionEnableArchiveAndMuteNewChats";

/// What the account knows about the setting besides its value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ArchiveMute {
    /// TDLib option `can_archive_and_mute_new_chats_from_unknown_users`
    /// (tdesktop app config `autoarchive_setting_available`).
    pub available: bool,
    /// The server reported the setting on this session, so the switch
    /// stays offered after it is turned off.
    pub seen_on: bool,
}

impl ArchiveMute {
    /// Folds one `updateOption`.
    pub fn apply_option(&mut self, name: &str, value: &OptionValue) {
        if name == "can_archive_and_mute_new_chats_from_unknown_users" {
            self.available = matches!(value, OptionValue::Boolean(true));
        }
    }

    /// Notes the settings the server reported.
    pub fn note_settings(&mut self, settings: &ArchiveChatListSettings) {
        self.seen_on |= settings.archive_and_mute_new_chats_from_unknown_users;
    }

    /// Whether the switch is offered: the server allows it, it is or was
    /// on, or the account has Premium.
    pub fn switch_shown(&self, settings: &ArchiveChatListSettings, premium: bool) -> bool {
        self.available
            || self.seen_on
            || settings.archive_and_mute_new_chats_from_unknown_users
            || premium
    }
}

/// Whether "Hide new chats?" is due: the server suggests it and the
/// setting, once known, is off.
pub fn suggestion_due(
    actions: &BTreeSet<String>,
    settings: Option<&ArchiveChatListSettings>,
) -> bool {
    actions.contains(ACTION)
        && settings.is_some_and(|settings| !settings.archive_and_mute_new_chats_from_unknown_users)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(on: bool) -> ArchiveChatListSettings {
        ArchiveChatListSettings {
            archive_and_mute_new_chats_from_unknown_users: on,
            ..Default::default()
        }
    }

    #[test]
    fn the_switch_is_offered_when_allowed_on_or_premium() {
        let mut state = ArchiveMute::default();
        assert!(!state.switch_shown(&settings(false), false));
        assert!(state.switch_shown(&settings(false), true), "premium");
        assert!(state.switch_shown(&settings(true), false), "on");
        state.apply_option(
            "can_archive_and_mute_new_chats_from_unknown_users",
            &OptionValue::Boolean(true),
        );
        assert!(state.switch_shown(&settings(false), false), "allowed");
        state.apply_option(
            "can_archive_and_mute_new_chats_from_unknown_users",
            &OptionValue::Boolean(false),
        );
        assert!(!state.switch_shown(&settings(false), false));
    }

    #[test]
    fn once_reported_on_the_switch_stays_after_turning_it_off() {
        let mut state = ArchiveMute::default();
        state.note_settings(&settings(true));
        assert!(state.switch_shown(&settings(false), false));
        state.note_settings(&settings(false));
        assert!(state.switch_shown(&settings(false), false));
    }

    #[test]
    fn the_suggestion_waits_for_the_settings_and_skips_when_on() {
        let actions: BTreeSet<String> = [ACTION.to_string()].into();
        assert!(!suggestion_due(&actions, None), "settings unknown");
        assert!(suggestion_due(&actions, Some(&settings(false))));
        assert!(
            !suggestion_due(&actions, Some(&settings(true))),
            "already on"
        );
        assert!(!suggestion_due(&BTreeSet::new(), Some(&settings(false))));
    }
}
