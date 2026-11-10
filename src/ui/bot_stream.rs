//! A bot's streaming reply (`updatePendingMessage`) shown the way
//! Telegram Desktop shows it: as the bot's next incoming message at the
//! end of the history, growing as drafts arrive. New text is revealed
//! smoothly over a few frames (like Zed's streaming agent output)
//! instead of jumping by whole draft chunks.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::*;
use quill::ids::MessageId;
use quill::lang::{keys, tr};
use quill::state::{HistoryMessage, unix_ms_now};
use quill::telegram::envelope::{
    ChatKind, MessageContent, MessageSender, RichMessageContent, TextContent,
};
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
        text: text.text.get(..end).unwrap_or_default().to_string(),
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
        animated: None,
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
            .messages
            .pending_bot_messages
            .get(&(chat_id.0, topic))
            .map(|draft| (topic, draft)),
        None => session
            .messages
            .pending_bot_messages
            .iter()
            .filter(|((chat, _), _)| *chat == chat_id.0)
            .max_by_key(|(_, draft)| draft.expires_at_ms)
            .map(|((_, topic), draft)| (*topic, draft)),
    }
}

/// A streaming draft's rich content as the history shows it. A draft has
/// no server message for callback buttons, a full-message fetch or a
/// quote (its id is negative), so those are dropped.
fn stream_rich(rich: &RichMessageContent) -> RichMessageContent {
    let mut rich = rich.clone();
    rich.is_full = true;
    rich.quote_texts = Default::default();
    rich.blocks
        .retain(|block| !matches!(block, quill::rich::RichBlock::ButtonRow { .. }));
    for block in &mut rich.blocks {
        if let quill::rich::RichBlock::Paragraph { buttons, .. } = block {
            buttons.clear();
        }
    }
    rich
}

/// The part of the rows key that moves with the draft: the clock while a
/// text reveal runs (a fresh value every frame rebuilds the growing row),
/// and otherwise a digest of the content, so a rich or empty draft changes
/// the key when its content does and not on every frame.
fn reveal_tick(
    draft_id: i64,
    content: &MessageContent,
    reveal: &StreamReveal,
    now_nanos: impl FnOnce() -> u128,
) -> u128 {
    use std::hash::{Hash, Hasher};
    match content {
        MessageContent::Text(text) if !text.text.is_empty() => {
            let total = text.text.chars().count();
            if reveal.draft_id != draft_id || (reveal.shown as usize) < total {
                now_nanos()
            } else {
                0
            }
        }
        other => {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            format!("{other:?}").hash(&mut hasher);
            u128::from(hasher.finish())
        }
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
        let tick = reveal_tick(
            draft.draft_id,
            &draft.content,
            &self.history.stream_reveal.borrow(),
            || {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |elapsed| elapsed.as_nanos())
            },
        );
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
                    .history
                    .stream_reveal
                    .borrow_mut()
                    .advance(draft.draft_id, total);
                if shown < total {
                    self.request_animation_tick(60, cx);
                }
                MessageContent::Text(reveal_text(text, shown))
            }
            MessageContent::RichMessage(rich) => MessageContent::RichMessage(stream_rich(rich)),
            // No text yet: Telegram's "Thinking…" placeholder.
            _ => MessageContent::Text(TextContent {
                text: tr(keys::Q_LEFTOVERS_THINKING).into(),
                entities: Vec::new(),
                link_preview: None,
                animated: None,
            }),
        };
        Some(HistoryMessage {
            sender,
            id: MessageId(-1 - draft.draft_id.rem_euclid(1 << 48)),
            chat_id,
            is_outgoing: false,
            date: (unix_ms_now() / 1000) as i32,
            content,
            send: quill::state::SendState::Sent,
            reply_to: None,
            forward_info: None,
            extras: Default::default(),
            interaction_info: None,
            is_pinned: false,
            media_album_id: 0,
            reply_markup: None,
            self_destruct: None,
            auto_delete: None,
            author_signature: None,
            ephemeral: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{StreamReveal, reveal_text, reveal_tick, stream_rich};
    use quill::rich::QuoteRichText;
    use quill::telegram::envelope::{MessageContent, RichMessageContent, TextContent};
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
            animated: None,
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

    #[test]
    fn streaming_draft_carries_no_quote_texts() {
        let quote = QuoteRichText::Plain("quotable".into());
        let rich = RichMessageContent {
            blocks: Vec::new(),
            is_full: false,
            quote_texts: vec![quote].into(),
        };
        let shown = stream_rich(&rich);
        assert!(shown.quote_texts.is_empty());
        assert!(shown.is_full);
        assert_eq!(rich.quote_texts.len(), 1);
    }

    fn text(body: &str) -> MessageContent {
        MessageContent::Text(TextContent {
            text: body.into(),
            entities: Vec::new(),
            link_preview: None,
            animated: None,
        })
    }

    fn revealed(draft_id: i64, shown: f64) -> StreamReveal {
        StreamReveal {
            draft_id,
            shown,
            last: None,
        }
    }

    #[test]
    fn a_draft_with_no_text_keeps_one_key_until_its_content_changes() {
        let reveal = StreamReveal {
            draft_id: 0,
            shown: 0.0,
            last: None,
        };
        let empty = text("");
        let first = reveal_tick(7, &empty, &reveal, || 111);
        assert_eq!(first, reveal_tick(7, &empty, &reveal, || 222));
        assert_ne!(first, reveal_tick(7, &text("a"), &revealed(7, 0.0), || 333));
    }

    #[test]
    fn a_running_reveal_changes_the_key_every_frame_and_a_finished_one_does_not() {
        let body = text("hello");
        assert_eq!(reveal_tick(7, &body, &revealed(7, 2.0), || 1), 1);
        assert_eq!(reveal_tick(7, &body, &revealed(7, 2.0), || 2), 2);
        assert_eq!(reveal_tick(7, &body, &revealed(7, 5.0), || 3), 0);
        assert_eq!(reveal_tick(8, &body, &revealed(7, 5.0), || 4), 4);
    }
}
