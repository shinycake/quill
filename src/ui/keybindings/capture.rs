//! The Shortcuts page's rebinding: which row waits for a keystroke, a chord
//! that was refused, and whether the keymap holds this account's overrides.
#![deny(clippy::indexing_slicing)]

/// The rebinding capture. One value, so a row waiting for a key and a
/// refusal message are never both on the page.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(in crate::ui) enum Capture {
    #[default]
    Idle,
    /// The action with this id waits for a keystroke.
    Waiting(String),
    /// A capture was refused (fixed chrome or another rebindable action).
    /// The chip keeps showing the chord that is actually bound.
    Refused { action: String, message: String },
}

impl Capture {
    /// The action waiting for a keystroke.
    pub(in crate::ui) fn waiting(&self) -> Option<&str> {
        match self {
            Self::Waiting(id) => Some(id),
            _ => None,
        }
    }

    /// The refusal message of action `id`.
    pub(in crate::ui) fn refusal_of(&self, id: &str) -> Option<&str> {
        match self {
            Self::Refused { action, message } if action == id => Some(message),
            _ => None,
        }
    }
}

/// Whether the keymap carries the saved overrides of the account on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(in crate::ui) enum Keymap {
    /// Saved overrides still to apply once the live driver is ready.
    #[default]
    Stale,
    Applied,
}

/// Everything the Shortcuts page keeps besides its focus handle.
#[derive(Debug, Default)]
pub(in crate::ui) struct KeybindingUi {
    pub(in crate::ui) capture: Capture,
    pub(in crate::ui) keymap: Keymap,
}

impl KeybindingUi {
    /// Whether keys go to a capture. Shortcuts take keystrokes for
    /// rebinding only while their page is `shown`; a capture left behind
    /// by a page that is gone is dropped.
    pub(in crate::ui) fn capture_active(&mut self, shown: bool) -> bool {
        if !shown {
            self.capture = Capture::Idle;
        }
        shown && self.capture.waiting().is_some()
    }

    /// A new prefs root needs its own keymap, even when it has no overrides.
    pub(in crate::ui) fn invalidate_account(&mut self) {
        self.keymap = Keymap::Stale;
        self.capture = Capture::Idle;
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
    fn a_refusal_is_dropped_with_the_page_and_is_not_a_capture() {
        let mut ui = KeybindingUi {
            capture: Capture::Refused {
                action: "focus-composer".into(),
                message: "Conflict".into(),
            },
            ..KeybindingUi::default()
        };
        assert_eq!(ui.capture.refusal_of("focus-composer"), Some("Conflict"));
        assert_eq!(ui.capture.refusal_of("other"), None);
        assert!(!ui.capture_active(true));
        assert!(!ui.capture_active(false));
        assert_eq!(ui.capture, Capture::Idle);
    }

    #[test]
    fn account_change_invalidates_applied_bindings_and_capture() {
        let mut ui = KeybindingUi {
            capture: Capture::Waiting("focus-composer".into()),
            keymap: Keymap::Applied,
        };
        ui.invalidate_account();
        assert_eq!(ui.keymap, Keymap::Stale);
        assert_eq!(ui.capture, Capture::Idle);
    }
}
