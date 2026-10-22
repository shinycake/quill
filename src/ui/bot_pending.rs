//! Streaming bot reply display, using existing text and rich-block renderers.
#![deny(clippy::indexing_slicing)]
use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::state::BotsPurpose;
use quill::state::{RequestPurpose, unix_ms_now};

impl QuillApp {
    pub(super) fn pending_bot_reply(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        super::bot_stream::open_chat_draft(self.session()?)?;
        self.pending_bot_reply_body(cx)
    }

    /// `pending_bot_reply` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    #[inline(never)]
    fn pending_bot_reply_body(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let chat_id = session.open_chat?;
        let (topic_id, draft) = super::bot_stream::open_chat_draft(session)?;
        if draft.expires_at_ms <= unix_ms_now() {
            return None;
        }
        let draft_id = draft.draft_id;
        let can_stop = draft.can_stop;
        let failed = draft.stop_failed;
        let stopped = draft.stopped;
        let stopping = session.requests.has_purpose_for_chat(
            RequestPurpose::Bots(BotsPurpose::StopPendingMessage { topic_id, draft_id }),
            chat_id,
        );
        // The reply itself renders in the history as the bot's next
        // message (`bot_stream`); this bar only offers Stop.
        if !can_stop && !failed {
            return None;
        }
        let mut row = div()
            .id("pending-bot-reply")
            .flex()
            .items_center()
            .justify_center()
            .gap_2()
            .py_1();
        if can_stop {
            row = row.child(
                Button::new("stop-bot-draft")
                    .icon(gpui_kit::assets::IconName::CircleStop)
                    .label(if stopping {
                        tr(keys::Q_MEDIA_STOPPING)
                    } else if stopped {
                        tr(keys::Q_MEDIA_STOPPED)
                    } else {
                        tr(keys::Q_MEDIA_STOP_GENERATING)
                    })
                    .small()
                    .ghost()
                    .disabled(stopping || stopped)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.stop_bot_draft(chat_id, topic_id, draft_id, cx);
                    })),
            );
        }
        if failed {
            row = row.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_MEDIA_COULD_NOT_STOP_THE_REPLY)),
            );
        }
        Some(row.into_any_element())
    }

    fn stop_bot_draft(
        &mut self,
        chat_id: ChatId,
        topic_id: i32,
        draft_id: i64,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            self.connection.set_note(
                match live
                    .driver
                    .stop_pending_bot_message(chat_id, topic_id, draft_id)
                {
                    Ok(Some(_)) => StatusNote::silent(tr(keys::Q_MEDIA_STOPPING_BOT_REPLY)),
                    Ok(None) => StatusNote::silent(tr(keys::Q_MEDIA_THIS_REPLY_IS_NO_LONGER)),
                    Err(_) => StatusNote::error(tr(keys::Q_MEDIA_COULD_NOT_STOP_THE_BOT)),
                },
            );
        }
        cx.notify();
    }
}
