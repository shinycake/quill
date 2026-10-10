//! Streaming bot reply display, using existing text and rich-block renderers.
use super::app::QuillApp;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::state::BotsPurpose;
use quill::state::{RequestPurpose, unix_ms_now};

impl QuillApp {
    pub(super) fn pending_bot_reply(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
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
                        "Stopping…"
                    } else if stopped {
                        "Stopped"
                    } else {
                        "Stop generating"
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
                    .child("Could not stop the reply. Try again."),
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
            self.connection.status_note = match live
                .driver
                .stop_pending_bot_message(chat_id, topic_id, draft_id)
            {
                Ok(Some(_)) => "stopping bot reply…".into(),
                Ok(None) => "this reply is no longer stoppable".into(),
                Err(_) => "could not stop the bot reply".into(),
            };
        }
        cx.notify();
    }
}
