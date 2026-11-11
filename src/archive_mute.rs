//! "Archive and mute new chats from non-contacts": when the setting is
//! offered, and when to suggest it (pure, no UI).
//!
//! Telegram Desktop shows the switch in Privacy and Security and in the
//! archive settings box only while the server allows it, the setting is
//! already on, or the account has Premium (`SetupArchiveAndMute`,
//! `GlobalPrivacy::showArchiveAndMute`); once shown it stays. When the
//! server suggests it (TDLib `suggestedActionEnableArchiveAndMuteNewChats`),
//! tdesktop reloads the setting and, if it is off, asks "Hide new chats?"
//! with a button to the settings (`SessionController::suggestArchiveAndMute`);
//! closing the box dismisses the suggestion.
#![deny(clippy::indexing_slicing)]

use crate::ids::RequestId;
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
    /// The reload the suggestion asks for.
    pub reload: ArchiveReload,
}

/// Where the reload of the archive settings stands after the server
/// suggested "Hide new chats?". Every stage has a way out: the answer of
/// its own request (`note_settings`), the error answer and a send that
/// never left (`note_reload_failed`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ArchiveReload {
    /// Nothing asked, or the last reload answered.
    #[default]
    Idle,
    /// The suggestion arrived and the reload is about to go out
    /// (`note_sent`); the box waits for the fresh answer.
    Suggested,
    /// The reload went out as this request: only its answer (or its error)
    /// ends the wait, an older fetch that predates the suggestion does not.
    Reloading(RequestId),
    /// The reload failed. The cached copy (if any) decides the box, and
    /// the next suggestion reloads again.
    Failed,
}

impl ArchiveMute {
    /// Folds one `updateOption`.
    pub fn apply_option(&mut self, name: &str, value: &OptionValue) {
        if name == "can_archive_and_mute_new_chats_from_unknown_users" {
            self.available = matches!(value, OptionValue::Boolean(true));
        }
    }

    /// Notes the settings the server reported as the answer of `request`.
    /// An answer that predates the suggestion (another request) leaves the
    /// box waiting for its own reload.
    pub fn note_settings(
        &mut self,
        request: Option<RequestId>,
        settings: &ArchiveChatListSettings,
    ) {
        self.seen_on |= settings.archive_and_mute_new_chats_from_unknown_users;
        match self.reload {
            ArchiveReload::Suggested => {}
            ArchiveReload::Reloading(waiting) if request != Some(waiting) => {}
            _ => self.reload = ArchiveReload::Idle,
        }
    }

    /// The reload went out as `request`.
    pub fn note_sent(&mut self, request: RequestId) {
        if self.reload == ArchiveReload::Suggested {
            self.reload = ArchiveReload::Reloading(request);
        }
    }

    /// The reload failed (`request` answered with an error, or `None`: the
    /// request could not be sent or its entry is gone): stop waiting for
    /// it. Another request's error changes nothing.
    pub fn note_reload_failed(&mut self, request: Option<RequestId>) {
        match self.reload {
            ArchiveReload::Suggested => self.reload = ArchiveReload::Failed,
            ArchiveReload::Reloading(waiting) if request.is_none_or(|id| id == waiting) => {
                self.reload = ArchiveReload::Failed;
            }
            _ => {}
        }
    }

    /// Folds the actions `updateSuggestedActions` added: a new
    /// archive-and-mute suggestion waits for the setting to reload.
    pub fn note_suggested(&mut self, added: &[String]) {
        if added.iter().any(|action| action == ACTION) {
            self.reload = ArchiveReload::Suggested;
        }
    }

    /// Whether the switch is offered: the server allows it, it is or was
    /// on, or the account has Premium.
    pub fn switch_shown(&self, settings: &ArchiveChatListSettings, premium: bool) -> bool {
        self.available
            || self.seen_on
            || settings.archive_and_mute_new_chats_from_unknown_users
            || premium
    }

    /// Whether "Hide new chats?" is due: the server suggests it and the
    /// setting, reloaded since the suggestion arrived, is off.
    pub fn suggestion_due(
        &self,
        actions: &BTreeSet<String>,
        settings: Option<&ArchiveChatListSettings>,
    ) -> bool {
        matches!(self.reload, ArchiveReload::Idle | ArchiveReload::Failed)
            && actions.contains(ACTION)
            && settings
                .is_some_and(|settings| !settings.archive_and_mute_new_chats_from_unknown_users)
    }
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
        state.note_settings(None, &settings(true));
        assert!(state.switch_shown(&settings(false), false));
        state.note_settings(None, &settings(false));
        assert!(state.switch_shown(&settings(false), false));
    }

    #[test]
    fn the_suggestion_waits_for_the_settings_and_skips_when_on() {
        let actions: BTreeSet<String> = [ACTION.to_string()].into();
        let state = ArchiveMute::default();
        assert!(!state.suggestion_due(&actions, None), "settings unknown");
        assert!(state.suggestion_due(&actions, Some(&settings(false))));
        assert!(
            !state.suggestion_due(&actions, Some(&settings(true))),
            "already on"
        );
        assert!(!state.suggestion_due(&BTreeSet::new(), Some(&settings(false))));
    }

    #[test]
    fn a_failed_reload_stops_waiting_and_the_next_suggestion_asks_again() {
        let actions: BTreeSet<String> = [ACTION.to_string()].into();
        let mut state = ArchiveMute::default();
        state.note_suggested(&[ACTION.to_string()]);
        state.note_sent(RequestId(7));
        assert!(!state.suggestion_due(&actions, Some(&settings(false))));
        state.note_reload_failed(Some(RequestId(3)));
        assert_eq!(
            state.reload,
            ArchiveReload::Reloading(RequestId(7)),
            "another request's error does not end the wait"
        );
        state.note_reload_failed(Some(RequestId(7)));
        assert_eq!(state.reload, ArchiveReload::Failed);
        assert!(
            state.suggestion_due(&actions, Some(&settings(false))),
            "the cached copy decides once the reload failed"
        );
        state.note_suggested(&[ACTION.to_string()]);
        assert_eq!(state.reload, ArchiveReload::Suggested);
    }

    #[test]
    fn a_send_that_never_left_stops_waiting() {
        let actions: BTreeSet<String> = [ACTION.to_string()].into();
        let mut state = ArchiveMute::default();
        state.note_suggested(&[ACTION.to_string()]);
        state.note_reload_failed(None);
        assert_eq!(state.reload, ArchiveReload::Failed);
        assert!(state.suggestion_due(&actions, Some(&settings(false))));
    }

    #[test]
    fn an_older_fetch_answer_does_not_end_the_wait_for_the_reload() {
        let actions: BTreeSet<String> = [ACTION.to_string()].into();
        let mut state = ArchiveMute::default();
        state.note_suggested(&[ACTION.to_string()]);
        state.note_sent(RequestId(9));
        state.note_settings(Some(RequestId(4)), &settings(false));
        assert!(
            !state.suggestion_due(&actions, Some(&settings(false))),
            "the answer predates the suggestion"
        );
        state.note_settings(Some(RequestId(9)), &settings(false));
        assert!(state.suggestion_due(&actions, Some(&settings(false))));
    }

    #[test]
    fn a_new_suggestion_waits_for_a_fresh_copy() {
        let actions: BTreeSet<String> = [ACTION.to_string()].into();
        let mut state = ArchiveMute::default();
        state.note_suggested(&["suggestedActionUpgradePremium".to_string()]);
        assert_eq!(
            state.reload,
            ArchiveReload::Idle,
            "other actions don't wait"
        );
        state.note_suggested(&[ACTION.to_string()]);
        state.note_sent(RequestId(1));
        assert!(
            !state.suggestion_due(&actions, Some(&settings(false))),
            "the cached copy is not enough"
        );
        state.note_settings(Some(RequestId(1)), &settings(true));
        assert!(
            !state.suggestion_due(&actions, Some(&settings(true))),
            "turned on elsewhere"
        );
        state.note_suggested(&[ACTION.to_string()]);
        state.note_sent(RequestId(2));
        state.note_settings(Some(RequestId(2)), &settings(false));
        assert!(state.suggestion_due(&actions, Some(&settings(false))));
    }
}
