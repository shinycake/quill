//! Tapping a big emoji plays a larger effect over the chat (tdesktop
//! `HistoryView::EmojiInteractions`).
//!
//! A tap on a lone emoji in a one-to-one chat asks TDLib for the effect
//! (`clickAnimatedEmojiMessage`); the other party's tap arrives as
//! `updateAnimatedEmojiMessageClicked`. Both land in
//! `Session::stickers.interactions`. This module starts them: a clip of the
//! effect sticker is drawn over the message, 5x the emoji's height, hugging
//! the side the bubble sits on, for as long as the sticker runs. The frame
//! comes from the clip's own clock (`LayeredClip::frame_now`), and the
//! window's frame clock keeps ticking while an effect plays.
//!
//! Where a message is on screen is noted while the history paints
//! (`anchor_tracker`); an effect for a message that is not visible is
//! dropped, as tdesktop does. The anchor is the emoji glyph at the start of
//! the text block, not the footer's reserved space. Like tdesktop, the
//! sticker is mirrored for messages on the left: its decoded frames are
//! flipped once when the effect starts and kept for the next one. Effects
//! past the fifth wait their turn (`may_start`, tdesktop `checkDelayed`).

use super::anim_layer::LayeredClip;
use super::app::QuillApp;
use super::premium_sticker_effect;
use super::sticker_playback::PlaybackSize;
use gpui_kit::*;
use quill::bubble_layout::LARGE_EMOJI_SIZE;
use quill::emoji_interaction::{EmojiEffect, MAX_PLAYS};
use quill::ids::{ChatId, MessageId};
use quill::state::Session;
use quill::telegram::envelope::MessageContent;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// tdesktop `kMaxDelay`: an effect whose sticker has not arrived by then
/// is not played any more.
const START_WAIT: Duration = Duration::from_secs(2);
/// tdesktop `kSmallDelay`: a new effect starts this long after the last.
const SMALL_DELAY: Duration = Duration::from_millis(200);
/// Effects waiting or playing, at most.
const MAX_QUEUED: usize = 8;
/// The glyph's line is this many times its size tall.
const GLYPH_LINE: f32 = 1.2;
/// Frame rate asked of the clock while an effect plays.
const TICK_FPS: u32 = 30;
/// The effect is this many emoji heights wide and tall, within bounds.
const SIZE_FACTOR: f32 = 5.0;
const MIN_SIDE: f32 = 200.0;
const MAX_SIDE: f32 = 360.0;
/// tdesktop `GenerateRandomShift`: up to 8% of the emoji's size.
const SHIFT_SHARE: f32 = 0.08;

type Key = (i64, i64);

thread_local! {
    /// Where each lone emoji was last painted (window coordinates), by chat
    /// and message id, as far as it shows in the history's clip.
    static ANCHORS: RefCell<HashMap<Key, Bounds<Pixels>>> = RefCell::new(HashMap::new());
    static ACTIVE: RefCell<Vec<Active>> = const { RefCell::new(Vec::new()) };
}

struct Active {
    effect: EmojiEffect,
    outgoing: bool,
    queued: Instant,
    /// The decoded sticker and when it began.
    clip: Option<LayeredClip>,
    seed: u64,
    /// A Premium sticker's effect (`premium_sticker_effect`): placed around
    /// the sticker, not an emoji.
    premium: bool,
}

/// Whether the Premium sticker effect of message `key` is playing or waiting.
pub(super) fn premium_playing(key: Key) -> bool {
    ACTIVE.with(|active| {
        active
            .borrow()
            .iter()
            .any(|a| a.premium && (a.effect.chat_id.0, a.effect.message_id.0) == key)
    })
}

/// Queue the effect of a Premium sticker. Dropped when too many wait.
pub(super) fn push_premium(effect: EmojiEffect, outgoing: bool) {
    ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        if active.len() >= MAX_QUEUED {
            return;
        }
        active.push(Active {
            seed: 0,
            outgoing,
            effect,
            queued: Instant::now(),
            clip: None,
            premium: true,
        });
    });
}

/// The history list is being rebuilt: forget where the emoji were.
pub(super) fn begin_pass() {
    ANCHORS.with(|anchors| anchors.borrow_mut().clear());
}

fn anchor(key: Key) -> Option<Bounds<Pixels>> {
    ANCHORS.with(|anchors| anchors.borrow().get(&key).copied())
}

/// An invisible child that notes where a lone emoji is painted.
fn anchor_tracker(key: Key) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |block, _, window, _| {
            // The emoji is the first thing in the block; the rest is the
            // time footer's reserved space.
            let glyph = LARGE_EMOJI_SIZE as f32;
            let bounds = Bounds::new(
                block.origin,
                size(
                    block.size.width.min(px(glyph)),
                    block.size.height.min(px(glyph * GLYPH_LINE)),
                ),
            );
            let visible = bounds.intersect(&window.content_mask().bounds);
            if visible.size.width > Pixels::ZERO && visible.size.height > Pixels::ZERO {
                ANCHORS.with(|anchors| {
                    anchors.borrow_mut().insert(key, bounds);
                });
            }
        },
    )
    .absolute()
    .inset_0()
    .size_full()
}

/// A lone emoji message (`msg_key` is chat id, message id) that plays an
/// effect when clicked.
pub(super) fn tappable(
    block: Stateful<Div>,
    msg_key: (i64, u64),
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let key = (msg_key.0, msg_key.1 as i64);
    block
        .relative()
        .child(anchor_tracker(key))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(move |this, event: &MouseUpEvent, window, cx| {
                if event.click_count == 1
                    && !this.selecting_in(ChatId(key.0))
                    && !gpui_kit::base::TextSelection::has_selection(window, cx)
                {
                    this.tap_animated_emoji(ChatId(key.0), MessageId(key.1));
                }
            }),
        )
        .into_any_element()
}

/// Where the effect goes: `side` square, hugging the bubble's outer side
/// like tdesktop's `computeRect`, centred on the emoji, moved by `shift`.
/// Returns the top-left corner. `anchor` is (left, top, right, bottom).
fn effect_origin(
    anchor: (f32, f32, f32, f32),
    side: f32,
    outgoing: bool,
    shift: (f32, f32),
) -> (f32, f32) {
    let (left, top, right, bottom) = anchor;
    let edge = side / 40.0;
    let x = if outgoing {
        right + edge - side
    } else {
        left - edge
    };
    let y = (top + bottom - side) / 2.0;
    (x + shift.0, y + shift.1)
}

/// tdesktop `checkDelayed`: whether another effect may start beside the
/// ones playing, given each one's elapsed and total time.
fn may_start(playing: &[(Duration, Duration)]) -> bool {
    if playing.len() >= MAX_PLAYS || playing.iter().any(|(at, _)| *at < SMALL_DELAY) {
        return false;
    }
    // Three or more at once: wait until one is half played.
    playing.len() < 3 || playing.iter().any(|(at, total)| *at * 2 > *total)
}

/// Mirror an RGBA/BGRA image left to right: `bytes` is `height` rows of
/// `width` 4-byte pixels.
fn flip_rows(bytes: &[u8], width: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    for row in bytes.chunks_exact(width * 4) {
        for pixel in row.as_chunks::<4>().0.iter().rev() {
            out.extend_from_slice(pixel);
        }
    }
    out
}

thread_local! {
    /// Mirrored frames by sticker file; two at most (frames are large).
    static MIRRORED: RefCell<Vec<(i32, Arc<[Arc<RenderImage>]>)>> = const { RefCell::new(Vec::new()) };
}

/// Every frame of `frames` mirrored left to right (a frame that cannot be
/// read stays as it is).
pub(super) fn flip_frames(frames: &[Arc<RenderImage>]) -> Arc<[Arc<RenderImage>]> {
    frames
        .iter()
        .map(|frame| {
            let width = frame.size(0).width.0.max(0) as u32;
            let height = frame.size(0).height.0.max(0) as u32;
            let flipped = frame.as_bytes(0).and_then(|bytes| {
                image::RgbaImage::from_raw(width, height, flip_rows(bytes, width as usize))
            });
            match flipped {
                Some(rgba) => Arc::new(RenderImage::new(smallvec::SmallVec::from_buf([
                    image::Frame::new(rgba),
                ]))),
                None => frame.clone(),
            }
        })
        .collect::<Vec<_>>()
        .into()
}

/// The frames of `file` mirrored, made once and kept.
fn mirrored_frames(file: i32, frames: &Arc<[Arc<RenderImage>]>) -> Arc<[Arc<RenderImage>]> {
    if let Some(found) = MIRRORED.with(|m| {
        m.borrow()
            .iter()
            .find(|(id, _)| *id == file)
            .map(|(_, frames)| frames.clone())
    }) {
        return found;
    }
    let flipped = flip_frames(frames);
    MIRRORED.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() >= 2 {
            let (_, old) = m.remove(0);
            super::image_budget::retire_all(old.iter().cloned());
        }
        m.push((file, flipped.clone()));
    });
    flipped
}

/// The effect's edge for an emoji `height` tall.
fn effect_side(height: f32) -> f32 {
    (height * SIZE_FACTOR).clamp(MIN_SIDE, MAX_SIDE)
}

/// A shift in `-SHIFT_SHARE..=SHIFT_SHARE` of `size` on each axis, from
/// `seed`.
fn shift_for(seed: u64, size: f32) -> (f32, f32) {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03;
    x ^= x >> 29;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 32;
    let unit = |bits: u64| (bits & 0xFFFF) as f32 / 65535.0 * 2.0 - 1.0;
    (
        unit(x) * SHIFT_SHARE * size,
        unit(x >> 16) * SHIFT_SHARE * size,
    )
}

fn message_is_outgoing(session: &Session, effect: &EmojiEffect) -> Option<bool> {
    session
        .histories
        .get(&effect.chat_id.0)
        .and_then(|history| history.messages.get(&effect.message_id.0))
        .map(|message| message.is_outgoing)
}

/// The emoji the message holds, for the "watching" action.
fn message_emoji(session: &Session, effect: &EmojiEffect) -> String {
    let text = session
        .histories
        .get(&effect.chat_id.0)
        .and_then(|history| history.messages.get(&effect.message_id.0))
        .and_then(|message| match &message.content {
            MessageContent::Text(text) => Some(text.text.trim().to_string()),
            _ => None,
        })
        .filter(|text| !text.is_empty());
    text.unwrap_or_else(|| effect.sticker.emoji.clone())
}

impl QuillApp {
    /// A lone emoji was clicked: ask TDLib for its effect. The answer is
    /// queued in the session and starts in [`Self::emoji_effect_overlay`].
    pub(super) fn tap_animated_emoji(&mut self, chat_id: ChatId, message_id: MessageId) {
        if let Some(live) = self.live.as_mut() {
            // A 404 (no effect for this emoji) comes back as an error that
            // needs no notice; the usual animation is all there is.
            let _ = live.driver.click_animated_emoji(chat_id, message_id);
        }
    }

    /// Start the effects the session queued and draw the ones playing, over
    /// the conversation. `None` when nothing plays.
    pub(super) fn emoji_effect_overlay(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let open = self.viewed_chat();
        self.premium_autoplay(cx);
        let queued = self
            .live
            .as_mut()
            .map(|live| live.driver.session.stickers.interactions.take_pending())
            .unwrap_or_default();
        if queued.is_empty() && ACTIVE.with(|active| active.borrow().is_empty()) {
            return None;
        }
        if !queued.is_empty() {
            let started = Instant::now();
            let outgoing: Vec<Option<bool>> = {
                let session = self.session();
                queued
                    .iter()
                    .map(|effect| session.and_then(|s| message_is_outgoing(s, effect)))
                    .collect()
            };
            ACTIVE.with(|active| {
                let mut active = active.borrow_mut();
                for (effect, outgoing) in queued.into_iter().zip(outgoing) {
                    // Only for a message on screen; a few play at once and
                    // the rest wait.
                    let key = (effect.chat_id.0, effect.message_id.0);
                    if anchor(key).is_none() || active.len() >= MAX_QUEUED {
                        continue;
                    }
                    active.push(Active {
                        seed: (effect.message_id.0 as u64) ^ (started.elapsed().as_nanos() as u64),
                        outgoing: outgoing.unwrap_or(false),
                        effect,
                        queued: started,
                        clip: None,
                        premium: false,
                    });
                }
            });
        }
        self.advance_effects(open, cx);
        let drawn: Vec<(Key, bool, u64, bool, LayeredClip)> = ACTIVE.with(|active| {
            active
                .borrow()
                .iter()
                .filter_map(|a| {
                    Some((
                        (a.effect.chat_id.0, a.effect.message_id.0),
                        a.outgoing,
                        a.seed,
                        a.premium,
                        a.clip.clone()?,
                    ))
                })
                .collect()
        });
        if ACTIVE.with(|active| active.borrow().is_empty()) {
            return None;
        }
        self.request_animation_tick_for(None, TICK_FPS, cx);
        Some(
            canvas(
                |_, _, _| {},
                move |_, _, window, _| {
                    for (key, outgoing, seed, premium, clip) in &drawn {
                        let (side, (x, y)) = if *premium {
                            let Some(sticker) = premium_sticker_effect::anchor(*key) else {
                                continue;
                            };
                            premium_sticker_effect::effect_rect(sticker, *outgoing)
                        } else {
                            let Some(anchor) = anchor(*key) else {
                                continue;
                            };
                            let height = f32::from(anchor.size.height);
                            let side = effect_side(height);
                            let origin = effect_origin(
                                (
                                    f32::from(anchor.left()),
                                    f32::from(anchor.top()),
                                    f32::from(anchor.right()),
                                    f32::from(anchor.bottom()),
                                ),
                                side,
                                *outgoing,
                                shift_for(*seed, height),
                            );
                            (side, origin)
                        };
                        let bounds = Bounds::new(point(px(x), px(y)), size(px(side), px(side)));
                        if let Some(shown) = &clip.shown {
                            shown.set(Instant::now());
                        }
                        if let Some(frame) = clip.frame_now().cloned() {
                            let _ = window.paint_image(
                                bounds,
                                bounds,
                                Corners::default(),
                                frame,
                                0,
                                false,
                            );
                        }
                    }
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .into_any_element(),
        )
    }

    /// Give each waiting effect its decoded sticker, tell the partner an
    /// incoming one is being watched, and drop what ended, timed out or
    /// belongs to another chat.
    fn advance_effects(&mut self, open: Option<ChatId>, cx: &mut Context<Self>) {
        let mut watched: Vec<(ChatId, String)> = Vec::new();
        let waiting: Vec<(usize, i32)> = ACTIVE.with(|active| {
            active
                .borrow()
                .iter()
                .enumerate()
                .filter(|(_, a)| a.clip.is_none())
                .map(|(i, a)| (i, a.effect.sticker.file_id.0))
                .collect()
        });
        let mut clips: Vec<(usize, Option<LayeredClip>)> = Vec::new();
        for (index, _) in &waiting {
            let sticker = ACTIVE.with(|active| active.borrow()[*index].effect.sticker.clone());
            let clip = self.effect_clip(&sticker, cx);
            clips.push((*index, clip));
        }
        ACTIVE.with(|active| {
            let mut active = active.borrow_mut();
            for (index, clip) in clips {
                let playing: Vec<(Duration, Duration)> = active
                    .iter()
                    .filter_map(|a| a.clip.as_ref())
                    .map(|c| (c.started.elapsed(), c.duration))
                    .collect();
                let entry = &mut active[index];
                if let Some(mut clip) = clip
                    && may_start(&playing)
                {
                    // Plays once from now, whatever the loop setting says;
                    // mirrored for a message on the left.
                    clip.started = Instant::now();
                    clip.looping = false;
                    if !entry.outgoing {
                        clip.frames = mirrored_frames(entry.effect.sticker.file_id.0, &clip.frames);
                    }
                    entry.clip = Some(clip);
                    if entry.effect.incoming {
                        watched.push((entry.effect.chat_id, entry.effect.sticker.emoji.clone()));
                    }
                }
            }
            active.retain(|a| {
                a.effect.chat_id == open.unwrap_or(ChatId(0))
                    && match &a.clip {
                        Some(clip) => clip.started.elapsed() < clip.duration,
                        None => a.queued.elapsed() < START_WAIT,
                    }
            });
        });
        for (chat_id, sticker_emoji) in watched {
            let emoji = ACTIVE
                .with(|active| {
                    active
                        .borrow()
                        .iter()
                        .find(|a| a.effect.chat_id == chat_id)
                        .map(|a| a.effect.clone())
                })
                .and_then(|effect| self.session().map(|s| message_emoji(s, &effect)))
                .unwrap_or(sticker_emoji);
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            if let Some(live) = self.live.as_mut() {
                let _ = live
                    .driver
                    .send_watching_animations(chat_id, &emoji, now_ms);
            }
        }
    }

    /// The decoded effect sticker; starts the download and decode.
    fn effect_clip(
        &self,
        sticker: &quill::telegram::envelope::StickerItem,
        cx: &mut Context<Self>,
    ) -> Option<LayeredClip> {
        self.layered_clip(
            sticker.file_id,
            sticker.format,
            PlaybackSize::Sticker,
            false,
            cx,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_SIDE, MIN_SIDE, SHIFT_SHARE, effect_origin, effect_side, flip_rows, may_start,
        shift_for,
    };
    use std::time::Duration;

    #[test]
    fn effect_hugs_the_bubble_side_and_centres_on_the_emoji() {
        let anchor = (100.0, 300.0, 148.0, 348.0);
        let side = 240.0;
        let (x, y) = effect_origin(anchor, side, false, (0.0, 0.0));
        assert_eq!(x, 100.0 - side / 40.0);
        assert_eq!(y, 324.0 - side / 2.0);
        let (x, _) = effect_origin(anchor, side, true, (0.0, 0.0));
        assert_eq!(x + side, 148.0 + side / 40.0);
    }

    #[test]
    fn effect_size_follows_the_emoji_within_bounds() {
        assert_eq!(effect_side(10.0), MIN_SIDE);
        assert_eq!(effect_side(48.0), 240.0);
        assert_eq!(effect_side(500.0), MAX_SIDE);
    }

    #[test]
    fn shift_stays_within_a_share_of_the_emoji() {
        for seed in 0..500 {
            let (x, y) = shift_for(seed, 50.0);
            assert!(x.abs() <= SHIFT_SHARE * 50.0 + 1e-4);
            assert!(y.abs() <= SHIFT_SHARE * 50.0 + 1e-4);
        }
        assert_ne!(shift_for(1, 50.0), shift_for(2, 50.0));
    }

    #[test]
    fn flip_mirrors_each_row() {
        // Two rows of three pixels; pixel i has bytes [i, i, i, i].
        let bytes: Vec<u8> = (0..6u8).flat_map(|i| [i; 4]).collect();
        let flipped = flip_rows(&bytes, 3);
        let pixels: Vec<u8> = flipped.chunks(4).map(|p| p[0]).collect();
        assert_eq!(pixels, vec![2, 1, 0, 5, 4, 3]);
        assert_eq!(flip_rows(&flipped, 3), bytes);
    }

    #[test]
    fn effects_wait_like_tdesktop_check_delayed() {
        let ms = Duration::from_millis;
        assert!(may_start(&[]));
        // Too soon after the last start.
        assert!(!may_start(&[(ms(100), ms(2000))]));
        assert!(may_start(&[(ms(300), ms(2000))]));
        // Three playing: one must be half played.
        let three = [
            (ms(300), ms(2000)),
            (ms(400), ms(2000)),
            (ms(500), ms(2000)),
        ];
        assert!(!may_start(&three));
        let half = [
            (ms(300), ms(2000)),
            (ms(400), ms(2000)),
            (ms(1500), ms(2000)),
        ];
        assert!(may_start(&half));
        // Five is the limit.
        assert!(!may_start(&[(ms(1500), ms(2000)); 5]));
    }
}
