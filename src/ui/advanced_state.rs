//! Advanced settings state: the typed-code buffer, the debug mode confirm and
//! the test environment flag (`ui/advanced_ui.rs`, `ui/secret_codes_ui.rs`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::*;
use quill::advanced::codes::CodeFeeder;

pub(crate) struct AdvancedUi {
    /// Keystrokes typed on the sign-in screen or the Settings list
    /// (tdesktop `CodesFeedString`).
    pub(super) codes: CodeFeeder,
    /// The `debugmode` confirm is on screen; holds whether the logs are on
    /// right now, which picks the question.
    pub(super) debug_confirm: Option<bool>,
    /// The active account connects to the test environment: the sign-in
    /// screen then says "Test Mode".
    pub(super) test_environment: bool,
}

impl AdvancedUi {
    /// Listens to every keystroke of the window for the secret codes; the
    /// handler decides whether a screen that takes them is showing.
    pub(super) fn new(cx: &mut Context<QuillApp>, load_disk_state: bool) -> Self {
        cx.observe_keystrokes(|this, event, window, cx| {
            this.feed_secret_code(&event.keystroke, window, cx)
        })
        .detach();
        let test_environment = load_disk_state
            && quill::settings::safe_app_root().is_some_and(|root| {
                quill::advanced::test_environment::account_is_test(
                    &root,
                    &quill::settings::active_account(&root),
                )
            });
        quill::advanced::test_environment::set_active_test(test_environment);
        // The update switch reads this at render time; read the file now.
        if load_disk_state {
            let _ = quill::advanced::update_channel::install_beta();
        }
        Self {
            codes: CodeFeeder::default(),
            debug_confirm: None,
            test_environment,
        }
    }
}
