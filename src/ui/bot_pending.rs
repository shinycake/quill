//! Streaming bot reply display, using existing text and rich-block renderers.
use super::app::QuillApp;
use super::message_text::{message_rich_block, rich_text_line};
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::rich::RichBlock;
use quill::state::{RequestPurpose, unix_ms_now};
use quill::telegram::envelope::MessageContent;

impl QuillApp {
    pub(super) fn pending_bot_reply(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let chat_id = session.open_chat?;
        let topic_id = session.open_topic.unwrap_or(0);
        let draft = session.pending_bot_messages.get(&(chat_id.0, topic_id))?;
        if draft.expires_at_ms <= unix_ms_now() {
            return None;
        }
        let draft_id = draft.draft_id;
        let can_stop = draft.can_stop;
        let failed = draft.stop_failed;
        let stopped = draft.stopped;
        let stopping = session.requests.has_purpose_for_chat(
            RequestPurpose::StopPendingMessage { topic_id, draft_id },
            chat_id,
        );
        let key = (
            chat_id.0 ^ i64::MIN,
            draft_id.unsigned_abs() & (u64::MAX >> 2),
        );
        let font = px(self.appearance.font_size_px as f32);
        let body = match &draft.content {
            MessageContent::Text(text) if !text.text.is_empty() => rich_text_line(
                &text.text,
                &text.entities,
                key,
                false,
                &self.spoiler_revealed,
                font,
                cx,
            ),
            MessageContent::RichMessage(rich) => {
                let mut rich = rich.clone();
                // Drafts have no server message ID for callback buttons or full-message fetches.
                rich.is_full = true;
                rich.blocks
                    .retain(|block| !matches!(block, RichBlock::ButtonRow { .. }));
                for block in &mut rich.blocks {
                    if let RichBlock::Paragraph { buttons, .. } = block {
                        buttons.clear();
                    }
                }
                message_rich_block(
                    key,
                    chat_id,
                    MessageId(0),
                    &rich,
                    &self.spoiler_revealed,
                    font,
                    cx,
                )
            }
            _ => div().child("Thinking…").into_any_element(),
        };
        let mut row = div()
            .id("pending-bot-reply")
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .max_h(px(260.))
            .overflow_y_scroll()
            .border_1()
            .border_color(cx.theme().border)
            .rounded_md()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(if stopped {
                        "Bot reply · stopped"
                    } else {
                        "Bot reply · generating"
                    }),
            )
            .child(body);
        if failed {
            row = row.child(
                div()
                    .text_xs()
                    .child("Could not stop the reply. Try again."),
            );
        }
        if can_stop {
            row = row.child(
                Button::new("stop-bot-draft")
                    .label(if stopping { "Stopping…" } else { "Stop" })
                    .disabled(stopping)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.stop_bot_draft(chat_id, topic_id, draft_id, cx);
                    })),
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
            self.status_note = match live
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
