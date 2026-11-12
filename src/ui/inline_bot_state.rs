//! The secret-chat inline-bot warning's confirmation (TGX
//! `TUTORIAL_INLINE_SEARCH_SECRECY`, which persists; Quill keeps it per
//! account on screen, a documented divergence). One value for every window
//! of the account: a chat window asks no more once the main window's box was
//! confirmed. The pending ask itself is per chat (`ChatUi::bot_alert`).
#![deny(clippy::indexing_slicing)]

/// The user's acknowledgement of the warning for this account: a fact the
/// user set once (no request, no stages), so it is a plain value.
#[derive(Default)]
pub(crate) struct InlineBotUi {
    acknowledged: bool,
}

impl crate::ui::on_screen::Farewell for InlineBotUi {}

impl InlineBotUi {
    pub(super) fn is_confirmed(&self) -> bool {
        self.acknowledged
    }

    pub(super) fn confirm(&mut self) {
        self.acknowledged = true;
    }
}
