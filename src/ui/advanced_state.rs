//! Advanced settings state: the typed-code buffer, the debug mode confirm and
//! the test environment flag (`ui/advanced_ui.rs`, `ui/secret_codes_ui.rs`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::*;
use quill::advanced::codes::CodeFeeder;
use std::cell::Cell;

pub(crate) struct AdvancedUi {
    /// Keystrokes typed on the sign-in screen or the Settings list
    /// (tdesktop `CodesFeedString`).
    pub(super) codes: CodeFeeder,
    /// The `debugmode` confirm is on screen; holds whether the logs are on
    /// right now, which picks the question.
    pub(super) debug_confirm: Option<bool>,
    /// Screenshot demos have no account store; their `testmode` switch lives
    /// here. A real app reads the environment from the store instead.
    demo_test_environment: bool,
    disk_backed: bool,
    /// Whether the clipboard holds an exported experimental-settings code,
    /// read once when the page opens and when the window is activated (a
    /// clipboard read asks the selection owner), not on every render.
    import_offer: Cell<Option<bool>>,
}

impl AdvancedUi {
    /// Listens to every keystroke of the window for the secret codes; the
    /// handler decides whether a screen that takes them is showing.
    pub(super) fn new(cx: &mut Context<QuillApp>, load_disk_state: bool) -> Self {
        cx.observe_keystrokes(|this, event, window, cx| {
            // A passcode typed on the lock screen is not a secret code.
            if !this.refused_while_locked() {
                this.feed_secret_code(&event.keystroke, window, cx)
            }
        })
        .detach();
        Self {
            codes: CodeFeeder::default(),
            debug_confirm: None,
            demo_test_environment: false,
            disk_backed: load_disk_state,
            import_offer: Cell::new(None),
        }
    }

    /// Whether Import shows: the clipboard is read only when no answer is
    /// kept (`read` says whether it holds a code).
    pub(super) fn import_offered(&self, read: impl FnOnce() -> bool) -> bool {
        self.import_offer.get().unwrap_or_else(|| {
            let offered = read();
            self.import_offer.set(Some(offered));
            offered
        })
    }

    /// The clipboard may have changed (the page opened, the window came to
    /// the front, Export wrote to it): read it again at the next render.
    pub(super) fn forget_import_offer(&self) {
        self.import_offer.set(None);
    }

    /// The active account connects to the test environment: the sign-in
    /// screen then says "Test Mode". Derived from the account store on every
    /// read.
    pub(super) fn test_environment(&self) -> bool {
        if self.disk_backed {
            quill::advanced::test_environment::active_is_test()
        } else {
            self.demo_test_environment
        }
    }

    pub(super) fn set_demo_test_environment(&mut self, on: bool) {
        self.demo_test_environment = on;
    }
}
