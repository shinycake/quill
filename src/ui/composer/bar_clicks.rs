//! Clicks on the edit and reply bars (tdesktop `FieldHeader`): the rules
//! are `quill::composer::bar_click_action`; this runs what they pick.

use super::*;
use crate::ui::composer_ui::BarClicks;
use quill::composer::{BarAction, BarKind};
use quill::reply_options::ReplyOption;
use std::rc::Rc;

impl QuillApp {
    /// Click handling for a bar. `jump_to` is the message a jump scrolls to
    /// (none for a scheduled edit, which is not in the history).
    pub(in crate::ui) fn bar_clicks(
        &self,
        kind: BarKind,
        jump_to: Option<MessageId>,
        cx: &mut Context<Self>,
    ) -> BarClicks {
        let owner = cx.entity().downgrade();
        BarClicks {
            kind,
            run: Rc::new(move |action, window, cx| {
                let _ = owner.update(cx, |this, cx| match action {
                    BarAction::JumpToMessage => {
                        if let Some(message_id) = jump_to {
                            this.jump_to_replied_message(message_id, cx);
                        }
                    }
                    BarAction::PickQuote => {
                        this.apply_reply_option(ReplyOption::UpdateQuote, window, cx);
                    }
                    BarAction::ReplyElsewhere => {
                        this.apply_reply_option(ReplyOption::ReplyInAnotherChat, window, cx);
                    }
                    BarAction::Nothing => {}
                });
            }),
        }
    }

    /// What a click on the reply bar can do for `reply`.
    pub(in crate::ui) fn reply_bar_kind(&self, reply: &ComposerReplyTo) -> BarKind {
        BarKind::Reply {
            has_text: self.reply_message_text(reply).is_some(),
            can_reply_elsewhere: self.reply_elsewhere_allowed(reply.chat_id, reply.message_id),
            same_chat: reply.target_chat.is_none() && self.open_chat_id() == Some(reply.chat_id),
        }
    }
}
