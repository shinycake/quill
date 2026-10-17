//! The reply and edit bars point at a message by id. When that message is
//! deleted the bar goes away (tdesktop `ComposeControls`: a destroyed
//! message cancels the edit or the reply). Text and preview are read from
//! the history when the bar draws, so there is nothing else to keep in step.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::*;
use quill::composer::ComposerReplyTo;
use quill::ids::ChatId;
use quill::telegram::SendReply;

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

    /// The reply a send carries. The only way a sender reads the reply: a
    /// reply to a deleted message is dropped first (tdesktop cancels the
    /// reply when the item is removed), so a delete that lands between two
    /// frames never reaches TDLib. `ComposerUi` keeps the reply private, so a
    /// new sender cannot skip this.
    pub(super) fn reply_for_send(&mut self, cx: &mut Context<Self>) -> Option<ComposerReplyTo> {
        self.cancel_gone_reply(cx);
        self.composer_ui.reply().cloned()
    }

    /// [`Self::reply_for_send`] as the request's reply target for `chat_id`
    /// (same-chat, or aimed here from another chat).
    pub(super) fn send_reply_for(
        &mut self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<SendReply> {
        self.reply_for_send(cx)
            .and_then(|reply| reply.send_target(chat_id))
    }

    /// The reply bar points at a message that is not in memory (a chat
    /// closed since keeps only its newest rows, a restored draft points
    /// anywhere): ask TDLib for it so the bar shows the real sender and
    /// text instead of "Loading...".
    pub(super) fn fetch_missing_reply_target(&mut self) {
        let Some((chat_id, message_id)) = self
            .composer_ui
            .reply()
            .map(|reply| (reply.chat_id, reply.message_id))
        else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            let _ = live
                .driver
                .fetch_composer_reply_target(chat_id, message_id);
        }
    }

    fn cancel_gone_reply(&mut self, cx: &mut Context<Self>) {
        let reply_gone =
            self.composer_ui
                .reply()
                .zip(self.session())
                .is_some_and(|(reply, session)| {
                    session.message_deleted(reply.chat_id, reply.message_id)
                });
        if reply_gone {
            self.clear_reply(cx);
        }
    }
}
