//! Messages of only emoji drawn as stickers, without a bubble (tdesktop
//! `UnwrappedMedia` around a `Sticker` or a `CustomEmoji`): a lone animated
//! emoji at 112 pt in its skin tone, which plays once the first time it
//! shows and again when tapped or when the other side's tap effect plays
//! on it, and one to three custom emoji at 112, 78 or 58 pt. Which message
//! gets which, and the sizes: `quill::animated_emoji`.
//!
//! The lone emoji's view is on a play head per message and replay
//! (`EmojiPlays::generation`), keyed by the id the message was sent with
//! (`EmojiInteractions::play_key`), so a message sent from here keeps
//! playing when it gets its final id. Once played, a view whose head the
//! player forgot shows the first frame without playing again until the
//! chat is left (`leave_sticker_chat` clears its part of the set).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::sticker_player::{HeadId, Part, StickerView, ViewAsk};
use gpui_kit::*;
use quill::animated_emoji::{EmojiMedia, EmojiPlay, MessageKey, STICKER_SIDE};
use quill::ids::{ChatId, FileId, MessageId};
use quill::local_path::sandboxed_display_path;
use quill::power_saving;
use quill::state::{FileMap, HistoryMessage, Session};
use quill::sticker_playback::player::{PlayMode, Rest};
use quill::telegram::envelope::{AnimatedEmoji, MessageContent};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// How `message`'s text is drawn big ("Large emoji" on), if it is.
pub(super) fn message_emoji_media(
    message: &HistoryMessage,
    session: Option<&Session>,
) -> Option<EmojiMedia> {
    let MessageContent::Text(text) = &message.content else {
        return None;
    };
    quill::animated_emoji::emoji_media(
        text,
        session.is_none_or(|s| s.settings.media_prefs.big_emoji),
    )
}

fn play_head(key: MessageKey, generation: u32) -> HeadId {
    HeadId::of(Part::Chat, ("animated-emoji", key.0, key.1, generation))
}

/// The head of a lone emoji that rests on its first frame.
fn rest_head(key: MessageKey) -> HeadId {
    HeadId::of(Part::Chat, ("animated-emoji-rest", key.0, key.1))
}

const FIRST_FRAME: PlayMode = PlayMode::At(Duration::ZERO);

/// The block of a message drawn as emoji stickers: the lone animated
/// emoji (`lone`, its view) or the custom emoji (`custom`, views by id).
/// `None` for other messages, which draw their text.
#[allow(clippy::too_many_arguments)]
pub(in crate::ui) fn message_block(
    message: &HistoryMessage,
    media: Option<&EmojiMedia>,
    lone: Option<&StickerView>,
    custom: &HashMap<i64, StickerView>,
    files: &FileMap,
    media_roots: &[PathBuf],
    session: Option<&Session>,
    cx: &mut Context<QuillApp>,
) -> Option<AnyElement> {
    let MessageContent::Text(text) = &message.content else {
        return None;
    };
    let msg_key = (message.chat_id.0, message.id.0 as u64);
    let media = media?;
    let block = match media {
        EmojiMedia::Sticker => {
            let animated = text.animated.as_ref()?;
            lone_emoji_block(msg_key, &text.text, animated, lone, files, media_roots)
        }
        EmojiMedia::CustomEmoji { lines } => {
            let side = px(media.side() as f32);
            let still = |id: i64| {
                custom_emoji_still(id, text.animated.as_ref(), session, files, media_roots)
            };
            custom_emoji_block(msg_key, lines, side, custom, still)
        }
        EmojiMedia::Glyphs => return None,
    };
    Some(if media.is_single(text) {
        super::emoji_effect::tappable(block, msg_key, None, cx)
    } else {
        block.into_any_element()
    })
}

/// The lone emoji, 112 pt square: its animation, else its thumbnail, else
/// the emoji itself while the sticker downloads.
fn lone_emoji_block(
    msg_key: (i64, u64),
    emoji: &str,
    animated: &AnimatedEmoji,
    view: Option<&StickerView>,
    files: &FileMap,
    media_roots: &[PathBuf],
) -> Stateful<Div> {
    let side = px(STICKER_SIDE as f32);
    let block = div()
        .id(("animated-emoji", msg_key.1))
        .mt_2()
        .flex_none()
        .w(side)
        .h(side);
    if let Some(view) = view {
        return block.child(super::anim_layer::frames(view.clone()).size_full());
    }
    let glyph = emoji.to_string();
    let fallback = move || {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(quill::bubble_layout::LARGE_EMOJI_SIZE as f32))
            .child(glyph.clone())
            .into_any_element()
    };
    let still = animated
        .sticker
        .display_file_id()
        .and_then(|id| files.get(&id.0))
        .and_then(|file| file.usable_path())
        .and_then(|path| sandboxed_display_path(path, media_roots));
    match still {
        Some(path) => block.child(
            img(path)
                .size_full()
                .object_fit(ObjectFit::Contain)
                .with_fallback(fallback),
        ),
        None => block.child(fallback()),
    }
}

/// The still of custom emoji `id`: the emoji panel's cache, else the
/// sticker a lone custom emoji came with.
fn custom_emoji_still(
    id: i64,
    animated: Option<&AnimatedEmoji>,
    session: Option<&Session>,
    files: &FileMap,
    media_roots: &[PathBuf],
) -> Option<ImageSource> {
    let cached = session
        .and_then(|s| s.stickers.emoji.custom_emoji_stickers.lookup(id))
        .and_then(|item| item.display_file_id());
    let own = animated
        .filter(|a| a.sticker.custom_emoji_id == Some(id))
        .and_then(|a| a.sticker.display_file_id());
    cached
        .or(own)
        .and_then(|file| files.get(&file.0))
        .and_then(|file| file.usable_path())
        .and_then(|path| sandboxed_display_path(path, media_roots))
        .map(ImageSource::from)
}

/// Custom emoji by line, each `side` square: animated, else still, else an
/// empty cell while it resolves.
fn custom_emoji_block(
    msg_key: (i64, u64),
    lines: &[Vec<i64>],
    side: Pixels,
    views: &HashMap<i64, StickerView>,
    still: impl Fn(i64) -> Option<ImageSource>,
) -> Stateful<Div> {
    div()
        .id(("big-custom-emoji", msg_key.1))
        .mt_2()
        .flex_none()
        .flex()
        .flex_col()
        .children(lines.iter().map(|line| {
            div().flex().children(line.iter().map(|id| {
                let cell = div().flex_none().w(side).h(side);
                if let Some(view) = views.get(id) {
                    cell.child(super::anim_layer::frames(view.clone()).size_full())
                } else if let Some(source) = still(*id) {
                    cell.child(img(source).size_full().object_fit(ObjectFit::Contain))
                } else {
                    cell
                }
            }))
        }))
}

impl QuillApp {
    /// The key `message`'s lone emoji plays under.
    fn emoji_play_key(&self, chat_id: ChatId, message_id: MessageId) -> MessageKey {
        let key = (chat_id.0, message_id.0);
        self.session()
            .map_or(key, |s| s.stickers.interactions.play_key(key))
    }

    /// The view of `message`'s lone animated emoji, in its skin tone. Power
    /// saving ("Animated emoji" in chats off) shows its first frame.
    pub(in crate::ui) fn lone_emoji_view(
        &self,
        message: &HistoryMessage,
        cx: &mut Context<QuillApp>,
    ) -> Option<StickerView> {
        if message_emoji_media(message, self.session()) != Some(EmojiMedia::Sticker) {
            return None;
        }
        let MessageContent::Text(text) = &message.content else {
            return None;
        };
        let animated = text.animated.as_ref()?;
        let key = self.emoji_play_key(message.chat_id, message.id);
        let file = animated.sticker.file_id;
        let (head, mode) = if power_saving::on(power_saving::Flag::EmojiChat) {
            (rest_head(key), FIRST_FRAME)
        } else {
            self.lone_emoji_play(key, file)
        };
        let side = STICKER_SIDE as f32;
        let ask = ViewAsk {
            fitz: animated.fitz,
            ..ViewAsk::sticker(file, animated.sticker.format, head, mode, side)
        };
        self.sticker_view(ask, cx)
    }

    /// Which head `key`'s view plays on, and how: once from the start until
    /// it played, then its first frame.
    fn lone_emoji_play(&self, key: MessageKey, file: FileId) -> (HeadId, PlayMode) {
        let Ok(mut plays) = self.playback.emoji_plays.try_borrow_mut() else {
            return (rest_head(key), FIRST_FRAME);
        };
        let head = play_head(key, plays.generation(key));
        let known = self.playback.stickers.head_known(head, file.0);
        if known == Some(true) {
            plays.started(key);
        }
        match plays.play(key, known.is_some()) {
            EmojiPlay::Play { generation } => {
                (play_head(key, generation), PlayMode::Once(Rest::First))
            }
            EmojiPlay::Rest => (rest_head(key), FIRST_FRAME),
        }
    }

    /// Play a lone animated emoji again (a tap, or the other side's tap
    /// effect starting on it), unless it is playing now.
    pub(in crate::ui) fn replay_lone_emoji(&self, chat_id: ChatId, message_id: MessageId) {
        let Some(file) = self
            .session()
            .and_then(|s| s.histories.get(&chat_id.0))
            .and_then(|history| history.messages.get(&message_id.0))
            .and_then(|message| match &message.content {
                MessageContent::Text(text) => text.animated.as_ref(),
                _ => None,
            })
            .map(|animated| animated.sticker.file_id)
        else {
            return;
        };
        let key = self.emoji_play_key(chat_id, message_id);
        let Ok(mut plays) = self.playback.emoji_plays.try_borrow_mut() else {
            return;
        };
        let head = play_head(key, plays.generation(key));
        let playing = self
            .playback
            .stickers
            .head_progress(head, file.0)
            .is_some_and(|(pos, total)| pos < total);
        if !playing {
            plays.replay(key);
        }
    }

    /// Views of a message of one to three custom emoji drawn big, by
    /// custom emoji id, each at the size the message's emoji take. `None`
    /// for other messages. Power saving ("Animated stickers" in chats off)
    /// leaves them still, like tdesktop's custom emoji stickers.
    pub(in crate::ui) fn big_custom_emoji_views(
        &self,
        message: &HistoryMessage,
        cx: &mut Context<QuillApp>,
    ) -> Option<HashMap<i64, StickerView>> {
        let media = message_emoji_media(message, self.session())?;
        let EmojiMedia::CustomEmoji { lines } = &media else {
            return None;
        };
        let mut out = HashMap::new();
        if power_saving::on(power_saving::Flag::StickersChat) {
            return Some(out);
        }
        let MessageContent::Text(text) = &message.content else {
            return Some(out);
        };
        let side = media.side() as f32;
        let mode = self.sticker_mode();
        for id in lines.iter().flatten() {
            if out.contains_key(id) {
                continue;
            }
            let cached = self
                .session()
                .and_then(|s| s.stickers.emoji.custom_emoji_stickers.lookup(*id))
                .map(|item| (item.file_id, item.format));
            let own = text
                .animated
                .as_ref()
                .filter(|a| a.sticker.custom_emoji_id == Some(*id))
                .map(|a| (a.sticker.file_id, a.sticker.format));
            let Some((file, format)) = cached.or(own) else {
                continue;
            };
            let head = HeadId::of(
                Part::Chat,
                ("big-custom-emoji", message.chat_id.0, message.id.0, *id),
            );
            if let Some(view) =
                self.sticker_view(ViewAsk::sticker(file, format, head, mode, side), cx)
            {
                out.insert(*id, view);
            }
        }
        Some(out)
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "animated_emoji_tests.rs"]
mod tests;
