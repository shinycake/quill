//! The secret-chat inline-bot warning's confirmation (TGX
//! `TUTORIAL_INLINE_SEARCH_SECRECY`, which persists; Quill keeps it per
//! account on screen, a documented divergence). One value for every window
//! of the account: a chat window asks no more once the main window's box was
//! confirmed. The pending ask itself is per chat (`ChatUi::bot_alert`).
#![deny(clippy::indexing_slicing)]

/// Whether the warning was confirmed for this account.
#[derive(Default)]
enum InlineBotNotice {
    #[default]
    Unconfirmed,
    Confirmed,
}

#[derive(Default)]
pub(crate) struct InlineBotUi {
    notice: InlineBotNotice,
}

impl crate::ui::on_screen::Farewell for InlineBotUi {}

impl InlineBotUi {
    pub(super) fn is_confirmed(&self) -> bool {
        matches!(self.notice, InlineBotNotice::Confirmed)
    }

    pub(super) fn confirm(&mut self) {
        self.notice = InlineBotNotice::Confirmed;
    }
}
