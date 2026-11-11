//! The Shortcuts page's rebinding: which row waits for a keystroke, a chord
//! that was refused, and whether the keymap holds this account's overrides.
#![deny(clippy::indexing_slicing)]

/// The rebinding capture: which row waits for a keystroke.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(in crate::ui) enum Capture {
    #[default]
    Idle,
    /// The action with this id waits for a keystroke.
    Waiting(String),
    /// Settings > Calls waits for the push-to-talk key.
    PushToTalk,
}

impl Capture {
    /// The action waiting for a keystroke.
    pub(in crate::ui) fn waiting(&self) -> Option<&str> {
        match self {
            Self::Waiting(id) => Some(id),
            Self::Idle | Self::PushToTalk => None,
        }
    }

    /// The Calls page waits for the push-to-talk key.
    pub(in crate::ui) fn push_to_talk_armed(&self) -> bool {
        matches!(self, Self::PushToTalk)
    }
}

/// Chords the page refused (fixed chrome or another rebindable action),
/// each under the row it was typed for. The chip keeps showing the chord
/// that is actually bound. A row's own next try, reset or success ends its
/// message; another row's capture leaves it alone.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(in crate::ui) struct Refusals(Vec<(String, String)>);

impl Refusals {
    /// The refusal message of action `id`.
    pub(in crate::ui) fn of(&self, id: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(action, _)| action == id)
            .map(|(_, message)| message.as_str())
    }

    /// `id` was refused with `message`, replacing its earlier one.
    pub(in crate::ui) fn refuse(&mut self, id: String, message: String) {
        self.clear_of(&id);
        self.0.push((id, message));
    }

    /// Action `id` starts over: only its own message goes.
    pub(in crate::ui) fn clear_of(&mut self, id: &str) {
        self.0.retain(|(action, _)| action != id);
    }
}

/// Whose saved overrides the keymap carries. It is stale for any other
/// account on screen, so an account change needs no reset by hand.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(in crate::ui) enum Keymap {
    /// No account's overrides applied yet: apply once the live driver is
    /// ready.
    #[default]
    Stale,
    Applied(quill::ids::AccountKey),
}

impl Keymap {
    /// The keymap already carries `account`'s overrides.
    pub(in crate::ui) fn carries(&self, account: &quill::ids::AccountKey) -> bool {
        matches!(self, Self::Applied(applied) if applied == account)
    }
}

/// Everything the Shortcuts page keeps besides its focus handle.
#[derive(Debug, Default)]
pub(in crate::ui) struct KeybindingUi {
    pub(in crate::ui) capture: Capture,
    pub(in crate::ui) refusals: Refusals,
    pub(in crate::ui) keymap: Keymap,
}

impl KeybindingUi {
    /// Whether keys go to a capture. Shortcuts take keystrokes for
    /// rebinding only while their page is `shown`; a capture left behind
    /// by a page that is gone is dropped.
    pub(in crate::ui) fn capture_active(&mut self, shown: bool) -> bool {
        if !shown {
            // The push-to-talk capture belongs to the Calls page, which
            // ends it itself.
            self.refusals = Refusals::default();
            if self.capture.waiting().is_some() {
                self.capture = Capture::Idle;
            }
        }
        shown && self.capture.waiting().is_some()
    }

    /// The page is left or locked: the capture and every message go.
    pub(in crate::ui) fn end(&mut self) {
        self.capture = Capture::Idle;
        self.refusals = Refusals::default();
    }

    /// A new prefs root: a capture for the old account's page is over. The
    /// keymap needs no reset: it names the account it carries.
    pub(in crate::ui) fn invalidate_account(&mut self) {
        self.end();
    }

    /// The Calls page's shortcut button: a second press disarms.
    pub(in crate::ui) fn toggle_ptt_capture(&mut self) {
        self.capture = if self.capture.push_to_talk_armed() {
            Capture::Idle
        } else {
            Capture::PushToTalk
        };
    }

    /// The Calls page is left or its key arrived: only a push-to-talk wait
    /// ends (a Shortcuts row keeps its own).
    pub(in crate::ui) fn end_ptt_capture(&mut self) {
        if self.capture.push_to_talk_armed() {
            self.capture = Capture::Idle;
        }
    }

    /// Row `id` starts waiting for a keystroke; its old message goes.
    pub(in crate::ui) fn start_capture(&mut self, id: String) {
        self.refusals.clear_of(&id);
        self.capture = Capture::Waiting(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_only_consumes_keys_while_shortcuts_are_shown_and_armed() {
        let mut ui = KeybindingUi::default();
        assert!(!ui.capture_active(true));
        ui.capture = Capture::Waiting("focus-composer".into());
        assert!(ui.capture_active(true));
        assert!(!ui.capture_active(false));
        assert_eq!(ui.capture, Capture::Idle);
    }

    #[test]
    fn the_push_to_talk_capture_is_one_stage_with_the_row_captures() {
        let mut ui = KeybindingUi::default();
        ui.toggle_ptt_capture();
        assert!(ui.capture.push_to_talk_armed() && ui.capture.waiting().is_none());
        // Shortcuts do not take its key, and leaving that page keeps it.
        assert!(!ui.capture_active(true));
        assert!(!ui.capture_active(false));
        assert!(ui.capture.push_to_talk_armed());
        // A row's capture replaces it; the Calls page ending its own wait
        // leaves the row's alone.
        ui.start_capture("focus-composer".into());
        assert!(!ui.capture.push_to_talk_armed());
        ui.end_ptt_capture();
        assert_eq!(ui.capture.waiting(), Some("focus-composer"));
        // The lock, a page change or an account change end whichever waits.
        ui.toggle_ptt_capture();
        ui.end();
        assert_eq!(ui.capture, Capture::Idle);
        ui.toggle_ptt_capture();
        ui.toggle_ptt_capture();
        assert_eq!(ui.capture, Capture::Idle, "a second press disarms");
    }

    #[test]
    fn a_refusal_is_dropped_with_the_page_and_is_not_a_capture() {
        let mut ui = KeybindingUi::default();
        ui.refusals
            .refuse("focus-composer".into(), "Conflict".into());
        assert_eq!(ui.refusals.of("focus-composer"), Some("Conflict"));
        assert_eq!(ui.refusals.of("other"), None);
        assert!(!ui.capture_active(true));
        assert!(!ui.capture_active(false));
        assert_eq!(ui.capture, Capture::Idle);
        assert_eq!(ui.refusals.of("focus-composer"), None);
    }

    #[test]
    fn starting_or_resetting_another_row_keeps_a_refusal() {
        let mut ui = KeybindingUi::default();
        ui.refusals.refuse("a".into(), "Conflict".into());
        ui.start_capture("b".into());
        assert_eq!(ui.refusals.of("a"), Some("Conflict"));
        ui.refusals.clear_of("b");
        assert_eq!(ui.refusals.of("a"), Some("Conflict"));
        ui.start_capture("a".into());
        assert_eq!(ui.refusals.of("a"), None, "its own try ends its message");
    }

    #[test]
    fn account_change_ends_the_capture_and_makes_the_keymap_stale_for_the_new_account() {
        let mut ui = KeybindingUi {
            capture: Capture::Waiting("focus-composer".into()),
            keymap: Keymap::Applied(quill::ids::AccountKey("a".into())),
            ..KeybindingUi::default()
        };
        ui.invalidate_account();
        assert!(!ui.keymap.carries(&quill::ids::AccountKey("b".into())));
        assert!(ui.keymap.carries(&quill::ids::AccountKey("a".into())));
        assert_eq!(ui.capture, Capture::Idle);
    }
}
