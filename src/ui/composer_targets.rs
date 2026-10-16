//! The reply and edit bars point at a message by id. When that message is
//! deleted the bar goes away (tdesktop `ComposeControls`: a destroyed
//! message cancels the edit or the reply). Text and preview are read from
//! the history when the bar draws, so there is nothing else to keep in step.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::*;

impl QuillApp {
    /// An edit comes before a reply: cancelling the edit hands the stashed
    /// reply back, which may be gone too.
    pub(super) fn cancel_gone_composer_targets(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let edit_gone = self
            .composer_ui
            .pending_edit
            .as_ref()
            .zip(self.session())
            .is_some_and(|(edit, session)| session.edit_target_gone(edit));
        if edit_gone {
            self.clear_edit(window, cx);
        }
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
