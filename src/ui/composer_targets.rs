//! The reply and edit bars point at a message by id. When that message is
//! deleted the bar goes away (tdesktop `ComposeControls`: a destroyed
//! message cancels the edit or the reply). Text and preview are read from
//! the history when the bar draws, so there is nothing else to keep in step.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::*;

impl QuillApp {
    /// An edit comes before a reply: cancelling the edit hands the stashed
    /// reply back, which may be gone too. Returns whether an edit was
    /// cancelled, so a submit that was about to save it can stop instead of
    /// sending the edit text as a new message.
    pub(super) fn cancel_gone_composer_targets(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let edit_gone = self
            .composer_ui
            .pending_edit
            .as_ref()
            .zip(self.session())
            .is_some_and(|(edit, session)| session.edit_target_gone(edit));
        if edit_gone {
            self.clear_edit(window, cx);
        }
        self.cancel_gone_reply(cx);
        edit_gone
    }

    /// Drop a reply to a deleted message and keep the text (tdesktop
    /// cancels the reply when the item is removed). Every send re-checks it,
    /// so a delete that lands between two frames never reaches TDLib.
    pub(super) fn cancel_gone_reply(&mut self, cx: &mut Context<Self>) {
        let reply_gone = self
            .composer_ui
            .pending_reply
            .as_ref()
            .zip(self.session())
            .is_some_and(|(reply, session)| {
                session.message_deleted(reply.chat_id, reply.message_id)
            });
        if reply_gone {
            self.clear_reply(cx);
        }
    }
}
