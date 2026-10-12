//! Choosing a Premium sticker without Premium (tdesktop
//! `Window::ShowSendPremiumError`): nothing is sent, the Premium stickers
//! preview opens with that sticker first.

use super::app::QuillApp;
use gpui_kit::*;
use quill::ids::FileId;

impl QuillApp {
    /// Whether `file_id` is a Premium sticker this account cannot send. If
    /// so, the preview is already on its way.
    pub(super) fn premium_sticker_blocked(
        &mut self,
        file_id: FileId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session) = self.session() else {
            return false;
        };
        if session.my_is_premium() || !session.sticker_requires_premium(file_id) {
            return false;
        }
        let sticker = session.panel_sticker(file_id).cloned();
        self.show_premium_sticker_preview(sticker, cx);
        true
    }
}
