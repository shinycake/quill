//! A bot's streaming reply (`updatePendingMessage`) shown the way
//! Telegram Desktop shows it: as the bot's next incoming message at the
//! end of the history, growing as drafts arrive. New text is revealed
//! smoothly over a few frames (like Zed's streaming agent output)
//! instead of jumping by whole draft chunks.

use super::app::QuillApp;
use gpui_kit::*;
use quill::ids::MessageId;
use quill::state::{HistoryMessage, unix_ms_now};
use quill::telegram::envelope::{ChatKind, MessageContent, MessageSender, TextContent};
use std::time::Instant;

/// Reveal state of the streaming draft on screen.
#[derive(Default)]
pub(super) struct StreamReveal {
    draft_id: i64,
    /// Characters revealed so far (fractional: advances every frame).
    shown: f64,
    last: Option<Instant>,
}

impl StreamReveal {
    /// Characters to show of a `target`-character draft now, catching up
    /// on the backlog within about a third of a second (but never slower
    /// than 80 characters a second).
    fn advance(&mut self, draft_id: i64, target: usize) -> usize {
        let now = Instant::now();
        if self.draft_id != draft_id {
            *self = Self {
                draft_id,
                shown: 0.0,
                last: Some(now),
            };
        }
        let elapsed = self
            .last
            .map_or(0.0, |last| now.duration_since(last).as_secs_f64())
            .min(0.1);
        self.last = Some(now);
        let target = target as f64;
        let backlog = (target - self.shown).max(0.0);
        let speed = (backlog / 0.35).max(80.0);
        self.shown = (self.shown + speed * elapsed).min(target);
        self.shown as usize
    }
}

/// `text` cut to its first `chars` characters, entities clipped to match.
fn reveal_text(text: &TextContent, chars: usize) -> TextContent {
    let end = text
        .text
        .char_indices()
        .nth(chars)
        .map_or(text.text.len(), |(index, _)| index);
    TextContent {
        text: text.text[..end].to_string(),
        entities: text
            .entities
            .iter()
            .filter(|entity| entity.utf8_start < end)
            .map(|entity| {
                let mut entity = entity.clone();
                entity.utf8_end = entity.utf8_end.min(end);
                entity
            })
            .collect(),
        link_preview: None,
    }
}

/// The streaming draft for the open chat, with its topic. In a topic view,
/// that topic's draft; in the main history, the chat's draft in any thread
/// (a bot can answer within a thread of the private chat).
pub(super) fn open_chat_draft(
    session: &quill::state::Session,
) -> Option<(i32, &quill::state::PendingBotMessage)> {
    let chat_id = session.open_chat?;
    match session.open_topic {
        Some(topic) => session
            .pending_bot_messages
            .get(&(chat_id.0, topic))
            .map(|draft| (topic, draft)),
        None => session
            .pending_bot_messages
            .iter()
            .filter(|((chat, _), _)| *chat == chat_id.0)
            .max_by_key(|(_, draft)| draft.expires_at_ms)
            .map(|((_, topic), draft)| (*topic, draft)),
    }
}

impl QuillApp {
    /// What the history rows key needs to know about a streaming reply:
    /// the draft and how much of it shows; while the reveal runs, a fresh
    /// value every frame, so each frame rebuilds the growing row.
    pub(super) fn stream_rows_hash(&self) -> Option<(i64, usize, u128)> {
        let (_, draft) = open_chat_draft(self.session()?)?;
        let total = match &draft.content {
            MessageContent::Text(text) => text.text.chars().count(),
            _ => 0,
        };
        let reveal = self.stream_reveal.borrow();
        let revealing = reveal.draft_id != draft.draft_id || (reveal.shown as usize) < total;
        let tick = if revealing {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos())
        } else {
            0
        };
        Some((draft.draft_id, total, tick))
    }

    /// The open chat's streaming bot reply as a history message (negative
    /// id: it can't be reacted to, pinned or replied to), or `None`.
    pub(super) fn streaming_bot_message(&self, cx: &mut Context<Self>) -> Option<HistoryMessage> {
        let session = self.session()?;
        let chat_id = session.open_chat?;
        let (_, draft) = open_chat_draft(session)?;
        if draft.expires_at_ms <= unix_ms_now() {
            return None;
        }
        let sender = match session.chats.get(&chat_id.0).map(|chat| &chat.kind) {
            Some(ChatKind::Private { user_id }) => Some(MessageSender::User { user_id: user_id.0 }),
            _ => None,
        };
        let content = match &draft.content {
            MessageContent::Text(text) if !text.text.is_empty() => {
                let total = text.text.chars().count();
                let shown = self
                    .stream_reveal
                    .borrow_mut()
                    .advance(draft.draft_id, total);
                if shown < total {
                    self.request_animation_tick(60, cx);
                }
                MessageContent::Text(reveal_text(text, shown))
            }
            MessageContent::RichMessage(rich) => {
                // A draft has no server message for callback buttons or a
                // full-message fetch.
                let mut rich = rich.clone();
                rich.is_full = true;
                rich.blocks
                    .retain(|block| !matches!(block, quill::rich::RichBlock::ButtonRow { .. }));
                for block in &mut rich.blocks {
                    if let quill::rich::RichBlock::Paragraph { buttons, .. } = block {
                        buttons.clear();
                    }
                }
                MessageContent::RichMessage(rich)
            }
            // No text yet: Telegram's "Thinking…" placeholder.
            _ => MessageContent::Text(TextContent {
                text: "Thinking…".into(),
                entities: Vec::new(),
                link_preview: None,
            }),
        };
        Some(HistoryMessage {
            sender,
            id: MessageId(-1 - draft.draft_id.rem_euclid(1 << 48)),
            chat_id,
            is_outgoing: false,
            date: (unix_ms_now() / 1000) as i32,
            content,
            pending: false,
            reply_to: None,
            forward_info: None,
            interaction_info: None,
            is_pinned: false,
            media_album_id: 0,
            reply_markup: None,
            self_destruct: None,
            auto_delete: None,
            author_signature: None,
            failed: false,
            can_retry: false,
            ephemeral: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{StreamReveal, reveal_text};
    use quill::telegram::envelope::TextContent;
    use quill::text::{TextEntity, TextEntityKind};

    #[test]
    fn reveal_cuts_at_characters_and_clips_entities() {
        let text = TextContent {
            text: "héllo wörld".into(),
            entities: vec![
                TextEntity {
                    utf8_start: 0,
                    utf8_end: 6,
                    kind: TextEntityKind::Bold,
                },
                TextEntity {
                    utf8_start: 7,
                    utf8_end: 13,
                    kind: TextEntityKind::Italic,
                },
            ],
            link_preview: None,
        };
        let cut = reveal_text(&text, 3);
        assert_eq!(cut.text, "hél");
        assert_eq!(cut.entities.len(), 1);
        assert_eq!(cut.entities[0].utf8_end, cut.text.len());
        assert_eq!(reveal_text(&text, 99).text, text.text);
    }

    #[test]
    fn reveal_restarts_for_a_new_draft() {
        let mut reveal = StreamReveal::default();
        reveal.advance(1, 100);
        reveal.shown = 50.0;
        assert_eq!(reveal.advance(2, 100), 0);
    }
}
