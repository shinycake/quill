//! A bot's streaming reply (`updatePendingMessage`) shown the way
//! Telegram Desktop shows it (history_streamed_drafts.cpp): as the bot's
//! next incoming message at the end of the history, its text appearing
//! line by line (`appear`, drawn by `text_reveal`). The final message
//! takes the draft's place and finishes the reveal from where it was.
#![deny(clippy::indexing_slicing)]

#[path = "bot_stream_appear.rs"]
mod appear;

use super::app::QuillApp;
use super::text_reveal::{RevealGeometry, RevealPaint, RevealSink, TextReveal};
use appear::Appearing;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr};
use quill::state::{HistoryMessage, unix_ms_now};
use quill::telegram::envelope::{
    ChatKind, MessageContent, MessageSender, RichMessageContent, TextContent,
};
use std::time::Instant;

/// Where the streamed reply on screen is (tdesktop keeps the same state
/// on the draft's history item and its view).
#[derive(Debug, Default)]
pub(super) enum StreamReveal {
    /// No streamed reply is appearing.
    #[default]
    Idle,
    /// The open chat's draft is appearing.
    Streaming(Reveal),
    /// The draft's final message took its place and is finishing the
    /// reveal (`HistoryStreamedDrafts::adoptIncoming`).
    Finishing { message: MessageId, reveal: Reveal },
}

/// One streamed reply appearing.
#[derive(Debug)]
pub(super) struct Reveal {
    chat: ChatId,
    draft_id: i64,
    /// The text last drawn: the final message is matched against it.
    text: String,
    appearing: Appearing,
    sink: RevealSink,
}

impl Reveal {
    fn new(chat: ChatId, draft_id: i64, text: &str) -> Self {
        Self {
            chat,
            draft_id,
            text: text.to_string(),
            appearing: Appearing::new(),
            sink: std::rc::Rc::new(std::cell::RefCell::new(RevealGeometry::default())),
        }
    }

    fn take_text(&mut self, text: &str, finalizing: bool) {
        if self.text != text {
            self.text = text.to_string();
            self.appearing.text_changed(finalizing);
        }
    }
}

/// A history row the streamed reply may be drawn in.
#[derive(Clone, Copy, Debug)]
enum Row<'a> {
    /// The draft's own row.
    Draft {
        chat: ChatId,
        draft_id: i64,
        text: &'a str,
    },
    /// A message the bot sent. `newest`: the newest in its chat;
    /// `draft_live`: the chat still has its draft.
    Message {
        chat: ChatId,
        id: MessageId,
        text: &'a str,
        newest: bool,
        draft_live: bool,
    },
}

impl StreamReveal {
    /// The state once `row` is drawn.
    fn follow(self, row: Row<'_>) -> Self {
        match (self, row) {
            (
                Self::Streaming(mut reveal),
                Row::Draft {
                    chat,
                    draft_id,
                    text,
                },
            ) if reveal.chat == chat && reveal.draft_id == draft_id => {
                reveal.take_text(text, false);
                Self::Streaming(reveal)
            }
            (
                _,
                Row::Draft {
                    chat,
                    draft_id,
                    text,
                },
            ) => Self::Streaming(Reveal::new(chat, draft_id, text)),
            (
                Self::Streaming(mut reveal),
                Row::Message {
                    chat,
                    id,
                    text,
                    newest: true,
                    draft_live: false,
                },
            ) if reveal.chat == chat && common_prefix(&reveal.text, text) > 0 => {
                reveal.take_text(text, true);
                Self::Finishing {
                    message: id,
                    reveal,
                }
            }
            (state, _) => state,
        }
    }

    /// The reveal drawing `row`, if it is the one appearing.
    fn drawing(&mut self, row: Row<'_>) -> Option<&mut Reveal> {
        match (self, row) {
            (Self::Streaming(reveal), Row::Draft { draft_id, .. })
                if reveal.draft_id == draft_id =>
            {
                Some(reveal)
            }
            (Self::Finishing { message, reveal }, Row::Message { id, .. }) if *message == id => {
                Some(reveal)
            }
            _ => None,
        }
    }
}

/// Characters `a` and `b` start with in common (`CommonPrefixLength`).
fn common_prefix(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

/// The history row id of a draft: negative, so it can't be reacted to,
/// pinned or replied to.
fn draft_row_id(draft_id: i64) -> MessageId {
    MessageId(-1 - draft_id.rem_euclid(1 << 48))
}

/// The reveal's clock: the frame's time, or a recording's pinned one.
pub(super) fn now() -> Instant {
    #[cfg(feature = "demo-capture")]
    if let Some(pinned) = PINNED.with(std::cell::Cell::get) {
        return pinned;
    }
    Instant::now()
}

#[cfg(feature = "demo-capture")]
thread_local! {
    static PINNED: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
}

/// Pin the reveal's clock for a frame-by-frame recording (`None` frees it).
#[cfg(feature = "demo-capture")]
pub(super) fn pin_clock(at: Option<Instant>) {
    PINNED.with(|pinned| pinned.set(at));
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

impl QuillApp {
    /// What the history rows key needs to know about a streaming reply:
    /// the draft and its text. The reveal itself runs at draw time.
    pub(super) fn stream_rows_hash(&self) -> Option<(i64, u64)> {
        use std::hash::{Hash, Hasher};
        let (_, draft) = open_chat_draft(self.session()?)?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        if let MessageContent::Text(text) = &draft.content {
            text.text.hash(&mut hasher);
        }
        Some((draft.draft_id, hasher.finish()))
    }

    /// The open chat's streaming bot reply as a history message, or `None`.
    pub(super) fn streaming_bot_message(&self) -> Option<HistoryMessage> {
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
                MessageContent::Text(TextContent {
                    link_preview: None,
                    animated: None,
                    ..text.clone()
                })
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
            id: draft_row_id(draft.draft_id),
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

    /// How much of `message` shows this frame, when it is the streamed
    /// reply appearing; `None` draws it whole.
    /// `room` and `look` are the row's: the text wraps as in its bubble.
    pub(super) fn stream_text_reveal(
        &self,
        message: &HistoryMessage,
        room: super::history::bubble_width::BubbleRoom,
        look: &super::synthetic::BubbleLook,
        cx: &mut Context<Self>,
    ) -> Option<TextReveal> {
        if message.is_outgoing || !super::surface_reveal::animate(cx) {
            return None;
        }
        let MessageContent::Text(text) = &message.content else {
            return None;
        };
        let session = self.session()?;
        let live = open_chat_draft(session)
            .filter(|(_, draft)| matches!(draft.content, MessageContent::Text(_)))
            .map(|(_, draft)| draft.draft_id);
        let row = match live {
            Some(draft_id) if draft_row_id(draft_id) == message.id => Row::Draft {
                chat: message.chat_id,
                draft_id,
                text: &text.text,
            },
            _ => Row::Message {
                chat: message.chat_id,
                id: message.id,
                text: &text.text,
                newest: session
                    .chats
                    .get(&message.chat_id.0)
                    .and_then(|chat| chat.last_message.as_ref())
                    .is_some_and(|last| last.id == message.id),
                draft_live: live.is_some(),
            },
        };
        let mut state = self.history.stream_reveal.try_borrow_mut().ok()?;
        *state = std::mem::take(&mut *state).follow(row);
        let finishing = matches!(*state, StreamReveal::Finishing { .. });
        let reveal = state.drawing(row)?;
        let (lines, skip) = reveal
            .sink
            .try_borrow()
            .map(|geometry| (geometry.lines(), geometry.skip()))
            .unwrap_or_default();
        let frame = reveal.appearing.frame(lines, skip, now());
        let paint = RevealPaint {
            cut: frame.cut,
            sink: reveal.sink.clone(),
        };
        if frame.moving {
            self.request_animation_tick(super::frame_clock::MAX_TICK_FPS, cx);
        } else if finishing {
            *state = StreamReveal::Idle;
            return None;
        }
        Some(TextReveal {
            paint,
            height: px(frame.height),
            width: frame.width.map(px),
            wrap: super::history::bubble_width::content_for_outer(
                super::history::bubble_width::message_width_limit(
                    cx,
                    &message.content,
                    look.font,
                    look.plain,
                )
                .map_or(room.outer(), |width| f32::from(width) as i32),
                look.plain,
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Row, StreamReveal, common_prefix, stream_rich};
    use quill::ids::{ChatId, MessageId};
    use quill::rich::QuoteRichText;
    use quill::telegram::envelope::RichMessageContent;

    const CHAT: ChatId = ChatId(21);

    fn draft(draft_id: i64, text: &str) -> Row<'_> {
        Row::Draft {
            chat: CHAT,
            draft_id,
            text,
        }
    }

    fn message(id: i64, text: &str, newest: bool, draft_live: bool) -> Row<'_> {
        Row::Message {
            chat: CHAT,
            id: MessageId(id),
            text,
            newest,
            draft_live,
        }
    }

    #[test]
    fn a_draft_starts_streaming_and_keeps_its_reveal_as_it_grows() {
        let mut state = StreamReveal::Idle.follow(draft(7, "Hel"));
        assert!(state.drawing(draft(7, "Hel")).is_some());
        state = state.follow(draft(7, "Hello there"));
        let StreamReveal::Streaming(reveal) = &state else {
            panic!("still streaming");
        };
        assert_eq!(reveal.text, "Hello there");
        assert!(state.drawing(message(5, "Hello", true, false)).is_none());
    }

    #[test]
    fn a_new_draft_id_starts_over() {
        let state = StreamReveal::Idle
            .follow(draft(7, "one"))
            .follow(draft(8, "two"));
        assert!(matches!(&state, StreamReveal::Streaming(r) if r.draft_id == 8));
    }

    #[test]
    fn the_final_message_takes_the_drafts_place() {
        let mut state = StreamReveal::Idle.follow(draft(7, "Hello")).follow(message(
            310,
            "Hello there",
            true,
            false,
        ));
        assert!(matches!(
            &state,
            StreamReveal::Finishing { message, reveal }
                if *message == MessageId(310) && reveal.text == "Hello there"
        ));
        assert!(
            state
                .drawing(message(310, "Hello there", true, false))
                .is_some()
        );
    }

    #[test]
    fn older_or_unrelated_messages_are_not_adopted() {
        let start = || StreamReveal::Idle.follow(draft(7, "Hello"));
        // An older message of the bot, scrolled into view.
        let older = start().follow(message(300, "Hello again", false, false));
        assert!(matches!(older, StreamReveal::Streaming(_)));
        // The draft is still streaming.
        let live = start().follow(message(310, "Hello", true, true));
        assert!(matches!(live, StreamReveal::Streaming(_)));
        // Another text altogether.
        let other = start().follow(message(310, "Bye", true, false));
        assert!(matches!(other, StreamReveal::Streaming(_)));
    }

    #[test]
    fn common_prefix_counts_characters() {
        assert_eq!(common_prefix("héllo", "hélp"), 3);
        assert_eq!(common_prefix("", "a"), 0);
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
}
