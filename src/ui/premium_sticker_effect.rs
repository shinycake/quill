//! The effect around a Premium sticker (tdesktop `HistoryView::Sticker`
//! `hasPremiumEffect` / `EmojiInteractions::playPremiumEffect`).
//!
//! A sticker whose `full_type` carries a `premium_animation` plays that
//! animation around itself, once, when its message first shows in the open
//! chat, and again when the sticker is clicked. A click while the effect is
//! still playing shows the "Premium sticker" hint instead (tdesktop
//! `showPremiumStickerTooltip`). The animation is 1.49 times the sticker
//! (`kPremiumMultiplier`), centred on it, hugging the side the bubble sits on
//! and mirrored for messages on the left like the emoji effects it shares the
//! overlay with (`emoji_effect`).
//!
//! The history notes where each Premium sticker is painted (`tracker`). Paint
//! runs after the overlay is built, so the first note asks for one more frame;
//! the overlay then turns what has not played yet into effects once the
//! animation has downloaded and decoded.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::sticker_playback::PlaybackSize;
use crate::ui::status_note::StatusNote;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::emoji_interaction::EmojiEffect;
use quill::ids::{ChatId, FileId, MessageId};
use quill::lang::{keys, tr};
use quill::power_saving;
use quill::telegram::envelope::{StickerContent, StickerFormat, StickerItem};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// tdesktop `kPremiumMultiplier`: the effect is this many times the sticker.
const SIZE_FACTOR: f32 = 1.0 + 0.245 * 2.0;
/// tdesktop `kPremiumShift`: of the sticker's width, the effect reaches past
/// the sticker's outer edge by this much.
const EDGE_SHARE: f32 = 21.0 / 240.0;
/// A paint note older than this is from a sticker that is no longer shown.
const SEEN_FRESH: Duration = Duration::from_millis(1500);
/// Messages remembered as played, at most (the set resets per chat anyway).
const MAX_PLAYED: usize = 4096;
/// The hint a click shows while the effect plays (tdesktop
/// `lng_sticker_premium_text`).
pub(super) fn hint() -> &'static str {
    tr(keys::STICKER_PREMIUM_TEXT)
}

type Key = (i64, i64);

/// A Premium sticker as painted last.
#[derive(Clone, Copy)]
struct Seen {
    bounds: Bounds<Pixels>,
    outgoing: bool,
    animation: FileId,
    at: Instant,
}

thread_local! {
    static SEEN: RefCell<HashMap<Key, Seen>> = RefCell::new(HashMap::new());
    static PLAYED: RefCell<Played> = RefCell::new(Played::default());
}

/// Which messages already played their effect, for each chat a window
/// shows. A chat no window shows any more starts over when it shows again,
/// as a rebuilt list does in tdesktop (each window's history keeps its
/// own `_animatedStickersPlayed`).
#[derive(Default)]
struct Played {
    chats: HashMap<i64, HashSet<i64>>,
}

impl Played {
    /// Whether `key` still has its automatic first play.
    fn pending(&self, key: Key) -> bool {
        self.chats
            .get(&key.0)
            .is_none_or(|played| !played.contains(&key.1))
    }

    fn mark(&mut self, key: Key) {
        let played = self.chats.entry(key.0).or_default();
        if played.len() >= MAX_PLAYED {
            played.clear();
        }
        played.insert(key.1);
    }

    /// Forget the chats no window shows.
    fn keep_shown(&mut self, shown: &[i64]) {
        self.chats.retain(|chat, _| shown.contains(chat));
    }
}

/// What a click on a Premium sticker does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Click {
    /// Play the effect again.
    Replay,
    /// The effect is still playing: show the hint.
    Hint,
    /// Nothing (no set to speak of, tdesktop sets no click handler).
    Nothing,
}

pub(super) fn click_action(playing: bool, has_set: bool) -> Click {
    match (has_set, playing) {
        (false, _) => Click::Nothing,
        (true, true) => Click::Hint,
        (true, false) => Click::Replay,
    }
}

/// Where the effect goes for a sticker at `anchor` (left, top, right,
/// bottom): the side, and the top-left corner.
pub(super) fn effect_rect(anchor: (f32, f32, f32, f32), outgoing: bool) -> (f32, (f32, f32)) {
    let (left, top, right, bottom) = anchor;
    let (width, height) = (right - left, bottom - top);
    let side = width.max(height) * SIZE_FACTOR;
    let edge = width * EDGE_SHARE;
    let x = if outgoing {
        right + edge - side
    } else {
        left - edge
    };
    (side, (x, top + (height - side) / 2.0))
}

/// Note where a Premium sticker is painted. `size` is the sticker's box:
/// the block it sits in also holds its top margin.
pub(super) fn tracker(
    key: Key,
    outgoing: bool,
    animation: FileId,
    size: Size<Pixels>,
) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |block, _, window, _| {
            let bounds = Bounds::new(point(block.left(), block.bottom() - size.height), size);
            let visible = bounds.intersect(&window.content_mask().bounds);
            if visible.size.width <= Pixels::ZERO || visible.size.height <= Pixels::ZERO {
                return;
            }
            // A sticker that has not played, and was not noted on a recent
            // paint, needs another frame: the overlay already ran this one.
            // A fresh note does not ask again, so waiting on the download
            // does not spin.
            let kick = SEEN.with(|seen| {
                let mut seen = seen.borrow_mut();
                seen.retain(|_, s| s.at.elapsed() < Duration::from_secs(5));
                let stale = seen.get(&key).is_none_or(|s| s.at.elapsed() >= SEEN_FRESH);
                let pending = PLAYED.with(|played| played.borrow().pending(key));
                seen.insert(
                    key,
                    Seen {
                        bounds,
                        outgoing,
                        animation,
                        at: Instant::now(),
                    },
                );
                pending && stale
            });
            if kick {
                window.request_animation_frame();
            }
        },
    )
    .absolute()
    .inset_0()
    .size_full()
}

/// Click target around a sticker attachment. A tap opens the set; a Premium
/// sticker replays its effect instead, and notes where it was painted.
pub(super) fn wrap(
    row_id: u64,
    sticker: &StickerContent,
    pending: bool,
    chat_id: i64,
    message_id: i64,
    outgoing: bool,
    attachment: AnyElement,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let set_id = sticker.set_id;
    let premium = sticker.premium_animation.filter(|_| !pending);
    if premium.is_none() && (set_id == 0 || pending) {
        return attachment;
    }
    let (w, h) = super::message_media::sticker_box(sticker.width, sticker.height);
    div()
        .id(("sticker-open-set", row_id))
        .relative()
        .when(premium.is_some(), |this| this.flex_none().w(w))
        .cursor_pointer()
        .role(gpui_kit::Role::Button)
        .aria_label(if premium.is_some() {
            tr(keys::Q_MONEY_PLAY_PREMIUM_STICKER_EFFECT)
        } else {
            tr(keys::Q_MONEY_OPEN_STICKER_SET)
        })
        .on_click(cx.listener(move |this, _, _, cx| match premium {
            Some(animation) => this.premium_sticker_clicked(
                (chat_id, message_id),
                outgoing,
                animation,
                set_id != 0,
                cx,
            ),
            None => this.view_message_sticker_set(set_id, cx),
        }))
        .child(attachment)
        .when_some(premium, |this, animation| {
            this.child(tracker(
                (chat_id, message_id),
                outgoing,
                animation,
                size(w, h),
            ))
        })
        .into_any_element()
}

/// Where the sticker of `key` is, if it was painted just now.
pub(super) fn anchor(key: Key) -> Option<(f32, f32, f32, f32)> {
    SEEN.with(|seen| {
        seen.borrow()
            .get(&key)
            .filter(|s| s.at.elapsed() < SEEN_FRESH)
            .map(|s| {
                (
                    f32::from(s.bounds.left()),
                    f32::from(s.bounds.top()),
                    f32::from(s.bounds.right()),
                    f32::from(s.bounds.bottom()),
                )
            })
    })
}

fn effect_for(key: Key, animation: FileId) -> EmojiEffect {
    EmojiEffect {
        chat_id: ChatId(key.0),
        message_id: MessageId(key.1),
        sticker: StickerItem {
            custom_emoji_id: None,
            id: 0,
            set_id: 0,
            emoji: String::new(),
            width: 0,
            height: 0,
            format: StickerFormat::Tgs,
            file_id: animation,
            thumb_file_id: None,
            thumb_width: 0,
            thumb_height: 0,
            requires_premium: false,
            premium_animation: None,
        },
        incoming: false,
    }
}

impl QuillApp {
    /// A click on a Premium sticker: play its effect again, or show the
    /// hint while it still plays.
    pub(super) fn premium_sticker_clicked(
        &mut self,
        key: Key,
        outgoing: bool,
        animation: FileId,
        has_set: bool,
        cx: &mut Context<Self>,
    ) {
        // tdesktop does not call `elementStartPremium` while stickers in
        // chats are paused, so the click neither replays nor hints.
        if power_saving::on(power_saving::Flag::StickersChat) {
            return;
        }
        match click_action(super::emoji_effect::premium_playing(key), has_set) {
            Click::Replay => {
                super::emoji_effect::push_premium(
                    effect_for(key, animation),
                    outgoing,
                    cx.background_executor().now(),
                );
                cx.notify();
            }
            Click::Hint => self.set_status_note(StatusNote::info(hint()), cx),
            Click::Nothing => {}
        }
    }

    /// Start the first play of each Premium sticker that just came on
    /// screen, once its animation is ready. `layered_clip` is the existing
    /// animated-sticker path: it asks `ensure_sticker_playback` to download
    /// the TGS when it is not local, and returns a clip only once that file
    /// has been decoded. Playback is skipped while stickers in chats are
    /// paused, but the file is still requested (tdesktop
    /// `loadVideoThumbnail`).
    pub(super) fn premium_autoplay(&mut self, cx: &mut Context<Self>) {
        let paused = power_saving::on(power_saving::Flag::StickersChat);
        let Some(chat) = self.viewed_chat() else {
            return;
        };
        let shown = self.chats_on_screen();
        SEEN.with(|seen| seen.borrow_mut().retain(|key, _| shown.contains(&key.0)));
        PLAYED.with(|played| played.borrow_mut().keep_shown(&shown));
        let due: Vec<(Key, Seen)> = SEEN.with(|seen| {
            seen.borrow()
                .iter()
                .filter(|(key, s)| key.0 == chat.0 && s.at.elapsed() < SEEN_FRESH)
                .map(|(key, s)| (*key, *s))
                .collect()
        });
        for (key, seen) in due {
            if !PLAYED.with(|played| played.borrow().pending(key)) {
                continue;
            }
            if self
                .layered_clip(
                    seen.animation,
                    StickerFormat::Tgs,
                    PlaybackSize::Sticker,
                    false,
                    cx,
                )
                .is_some()
                && !paused
            {
                PLAYED.with(|played| played.borrow_mut().mark(key));
                super::emoji_effect::push_premium(
                    effect_for(key, seen.animation),
                    seen.outgoing,
                    cx.background_executor().now(),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Click, Played, SIZE_FACTOR, click_action, effect_rect};

    #[test]
    fn each_message_autoplays_once_per_chat_visit() {
        let mut played = Played::default();
        assert!(played.pending((1, 10)));
        played.mark((1, 10));
        assert!(!played.pending((1, 10)));
        assert!(played.pending((1, 11)));
        assert!(played.pending((2, 10)));
        played.mark((2, 10));
        assert!(!played.pending((1, 10)));
        played.keep_shown(&[2]);
        assert!(played.pending((1, 10)));
        assert!(!played.pending((2, 10)));
    }

    #[test]
    fn click_replays_or_hints_while_playing() {
        assert_eq!(click_action(false, true), Click::Replay);
        assert_eq!(click_action(true, true), Click::Hint);
        assert_eq!(click_action(false, false), Click::Nothing);
        assert_eq!(click_action(true, false), Click::Nothing);
    }

    #[test]
    fn effect_surrounds_the_sticker_on_the_bubble_side() {
        let anchor = (100.0, 200.0, 300.0, 400.0);
        let (side, (x, y)) = effect_rect(anchor, false);
        assert!((side - 200.0 * SIZE_FACTOR).abs() < 1e-3);
        // Centred vertically.
        assert!((y + side / 2.0 - 300.0).abs() < 1e-3);
        // Left message: reaches left of the sticker by the edge share.
        assert!(x < 100.0 && 100.0 - x < 30.0);
        let (side, (x, _)) = effect_rect(anchor, true);
        assert!(x + side > 300.0 && x + side - 300.0 < 30.0);
    }
}
