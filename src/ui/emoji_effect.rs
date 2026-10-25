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
#![deny(clippy::indexing_slicing)]

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
    /// On the executor clock (`cx.background_executor().now()`), like
    /// `missing`; a clip's `started` is on the real clock.
    queued: Instant,
    /// The decoded sticker and when it began.
    clip: Option<LayeredClip>,
    seed: u64,
    /// A Premium sticker's effect (`premium_sticker_effect`): placed around
    /// the sticker, not an emoji.
    premium: bool,
    /// A message effect (`message_effect_play`): centred on the message's
    /// effect badge.
    message: Option<MessageEffectPlay>,
    /// How long a sticker that has not arrived is waited for.
    wait: Duration,
}

/// What a message effect needs besides the sticker it plays.
#[derive(Clone, Copy)]
struct MessageEffectPlay {
    effect_id: i64,
    /// Where the badge was last seen, to find the message again when it
    /// gets its final id while the effect plays.
    last: Option<Point<Pixels>>,
    /// Since when the badge has not been on screen (executor clock).
    missing: Option<Instant>,
}

/// A message effect whose badge is off screen this long has ended
/// (tdesktop removes it once its view is gone).
const BADGE_GRACE: Duration = Duration::from_millis(400);
/// How far a moved badge is looked for.
const REBIND_RADIUS: f32 = 160.0;
/// `Sticker::MessageEffectSize`: twice the big emoji's 112 px.
const MESSAGE_EFFECT_SIDE: f32 = 224.0;

/// Whether a badge missing since `at` is still within its grace at `now`.
fn badge_within_grace(now: Instant, at: Instant) -> bool {
    now.saturating_duration_since(at) < BADGE_GRACE
}

/// Whether an effect queued at `queued` has waited `wait` for its sticker by `now`.
fn sticker_wait_over(now: Instant, queued: Instant, wait: Duration) -> bool {
    now.saturating_duration_since(queued) >= wait
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

/// Whether the effect of message `key`, sent with an effect, is playing
/// or waiting for its animation.
#[cfg(test)]
#[cfg(feature = "demo-capture")]
pub(super) fn message_effect_playing(key: Key) -> bool {
    ACTIVE.with(|active| {
        active
            .borrow()
            .iter()
            .any(|a| a.message.is_some() && (a.effect.chat_id.0, a.effect.message_id.0) == key)
    })
}

/// Queue the effect of a Premium sticker. Dropped when too many wait.
pub(super) fn push_premium(effect: EmojiEffect, outgoing: bool, now: Instant) {
    ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        if active.len() >= MAX_QUEUED {
            return;
        }
        active.push(Active {
            seed: 0,
            outgoing,
            effect,
            queued: now,
            clip: None,
            premium: true,
            message: None,
            wait: START_WAIT,
        });
    });
}

/// Queue the effect of a message sent with an effect, centred on its badge.
/// `wait` is how long its animation may take to arrive.
pub(super) fn push_message(
    effect: EmojiEffect,
    outgoing: bool,
    effect_id: i64,
    wait: Duration,
    now: Instant,
) {
    ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        if active.len() >= MAX_QUEUED {
            return;
        }
        active.push(Active {
            seed: 0,
            outgoing,
            effect,
            queued: now,
            clip: None,
            premium: false,
            message: Some(MessageEffectPlay {
                effect_id,
                last: None,
                missing: None,
            }),
            wait,
        });
    });
}

/// Follow each message effect of `chat` to its badge. A message gets its
/// final id while its effect plays (`updateMessageSendSucceeded`): the
/// badge is then found again by where it was. An effect whose badge is
/// gone for good ends.
fn follow_badges(chat: ChatId, now: Instant) {
    ACTIVE.with(|active| {
        for a in active.borrow_mut().iter_mut() {
            if a.effect.chat_id != chat {
                continue;
            }
            let Some(play) = a.message.as_mut() else {
                continue;
            };
            let key = (a.effect.chat_id.0, a.effect.message_id.0);
            if let Some(seen) = super::message_effect_badge::seen(key) {
                play.last = Some(seen.bounds.origin);
                play.missing = None;
                continue;
            }
            let moved = play.last.and_then(|last| {
                super::message_effect_badge::nearest(
                    key.0,
                    play.effect_id,
                    a.outgoing,
                    last,
                    px(REBIND_RADIUS),
                )
            });
            match moved {
                Some((new_key, seen)) => {
                    a.effect.message_id = MessageId(new_key.1);
                    play.last = Some(seen.bounds.origin);
                    play.missing = None;
                }
                None => {
                    play.missing.get_or_insert(now);
                }
            }
        }
    });
}

/// tdesktop `computeRect` for a message effect: centred on the badge's
/// picture, then moved a third of the effect's width toward the middle of
/// the chat (to the left for a message on the right). `badge` is the
/// picture's (left, top, right, bottom).
fn message_effect_origin(badge: (f32, f32, f32, f32), side: f32, outgoing: bool) -> (f32, f32) {
    let (left, top, right, bottom) = badge;
    let shift = if outgoing { -side / 3.0 } else { side / 3.0 };
    (
        shift + left + ((right - left) - side) / 2.0,
        top + ((bottom - top) - side) / 2.0,
    )
}

/// A history list is being rebuilt: forget where the emoji and effect
/// badges were, except those of the chats `keep` names (shown by another
/// window).
fn begin_pass(keep: &dyn Fn(i64) -> bool) {
    ANCHORS.with(|anchors| anchors.borrow_mut().retain(|key, _| keep(key.0)));
    super::message_effect_badge::begin_pass(keep);
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
    /// The history of the window being built is rebuilt: its lone emoji and
    /// effect badges are noted again as its rows paint. The notes of the
    /// chats other windows show stay.
    pub(super) fn begin_effect_notes_pass(&self) {
        let own = self.viewed_chat().map(|chat| chat.0);
        let others: Vec<i64> = self
            .chats_on_screen()
            .into_iter()
            .filter(|chat| Some(*chat) != own)
            .collect();
        begin_pass(&|chat| others.contains(&chat));
    }

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
    /// the conversation of the window being built (its `viewed_chat`; each
    /// window draws its own chat's). `window_active`: that window is the
    /// active one. `None` when nothing plays there.
    pub(super) fn emoji_effect_overlay(
        &mut self,
        window_active: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let open = self.viewed_chat()?;
        let now = cx.background_executor().now();
        self.premium_autoplay(cx);
        self.message_effects_pass(window_active, cx);
        let queued = self
            .live
            .as_mut()
            .map(|live| live.driver.session.stickers.interactions.take_pending())
            .unwrap_or_default();
        let none_here =
            || ACTIVE.with(|active| active.borrow().iter().all(|a| a.effect.chat_id != open));
        if queued.is_empty() && none_here() {
            return None;
        }
        if !queued.is_empty() {
            let started = now;
            let seed_clock = Instant::now();
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
                        seed: (effect.message_id.0 as u64)
                            ^ (seed_clock.elapsed().as_nanos() as u64),
                        outgoing: outgoing.unwrap_or(false),
                        effect,
                        queued: started,
                        clip: None,
                        premium: false,
                        message: None,
                        wait: START_WAIT,
                    });
                }
            });
        }
        follow_badges(open, now);
        let shown = self.chats_on_screen();
        self.advance_effects(open, &shown, now, cx);
        let drawn: Vec<(Key, bool, u64, bool, bool, LayeredClip)> = ACTIVE.with(|active| {
            active
                .borrow()
                .iter()
                .filter(|a| a.effect.chat_id == open)
                .filter_map(|a| {
                    Some((
                        (a.effect.chat_id.0, a.effect.message_id.0),
                        a.outgoing,
                        a.seed,
                        a.premium,
                        a.message.is_some(),
                        a.clip.clone()?,
                    ))
                })
                .collect()
        });
        if none_here() {
            return None;
        }
        self.request_animation_tick_for(None, TICK_FPS, cx);
        Some(
            canvas(
                |_, _, _| {},
                move |_, _, window, _| {
                    for (key, outgoing, seed, premium, message, clip) in &drawn {
                        let (side, (x, y)) = if *message {
                            let Some(badge) = super::message_effect_badge::seen(*key) else {
                                continue;
                            };
                            let bounds = badge.bounds;
                            (
                                MESSAGE_EFFECT_SIDE,
                                message_effect_origin(
                                    (
                                        f32::from(bounds.left()),
                                        f32::from(bounds.top()),
                                        f32::from(bounds.right()),
                                        f32::from(bounds.bottom()),
                                    ),
                                    MESSAGE_EFFECT_SIDE,
                                    *outgoing,
                                ),
                            )
                        } else if *premium {
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

    /// Give each waiting effect of `open` (the chat of the window being
    /// built) its decoded sticker, tell the partner an incoming one is being
    /// watched, and drop what ended or timed out there. The effects of a
    /// chat no window shows (`shown`) go; other windows' wait for their own
    /// pass.
    fn advance_effects(
        &mut self,
        open: ChatId,
        shown: &[i64],
        now: Instant,
        cx: &mut Context<Self>,
    ) {
        let mut watched: Vec<(ChatId, String)> = Vec::new();
        let waiting: Vec<(usize, i32)> = ACTIVE.with(|active| {
            active
                .borrow()
                .iter()
                .enumerate()
                .filter(|(_, a)| a.clip.is_none() && a.effect.chat_id == open)
                .map(|(i, a)| (i, a.effect.sticker.file_id.0))
                .collect()
        });
        let mut clips: Vec<(usize, Option<LayeredClip>)> = Vec::new();
        for (index, _) in &waiting {
            let Some(sticker) = ACTIVE.with(|active| {
                active
                    .borrow()
                    .get(*index)
                    .map(|a| a.effect.sticker.clone())
            }) else {
                continue;
            };
            let clip = self.effect_clip(&sticker, cx);
            clips.push((*index, clip));
        }
        ACTIVE.with(|active| {
            let mut active = active.borrow_mut();
            for (index, clip) in clips {
                // tdesktop `checkDelayed` counts one history's effects.
                let playing: Vec<(Duration, Duration)> = active
                    .iter()
                    .filter(|a| a.effect.chat_id == open)
                    .filter_map(|a| a.clip.as_ref())
                    .map(|c| (c.started.elapsed(), c.duration))
                    .collect();
                // `effect_clip` ran between collecting `index` and here; if
                // the list shrank meanwhile, skip rather than index past it.
                let Some(entry) = active.get_mut(index) else {
                    continue;
                };
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
                if a.effect.chat_id != open {
                    return shown.contains(&a.effect.chat_id.0);
                }
                a.message
                    .is_none_or(|play| play.missing.is_none_or(|at| badge_within_grace(now, at)))
                    && match &a.clip {
                        Some(clip) => clip.started.elapsed() < clip.duration,
                        None => !sticker_wait_over(now, a.queued, a.wait),
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
        BADGE_GRACE, MAX_SIDE, MIN_SIDE, SHIFT_SHARE, START_WAIT, badge_within_grace,
        effect_origin, effect_side, flip_rows, may_start, shift_for, sticker_wait_over,
    };
    use std::time::{Duration, Instant};

    #[test]
    fn a_missing_badge_ends_the_effect_at_the_grace() {
        let at = Instant::now();
        let ms = Duration::from_millis;
        assert!(badge_within_grace(at, at));
        assert!(badge_within_grace(at + ms(399), at));
        assert!(!badge_within_grace(at + BADGE_GRACE, at));
        // A clock that reads earlier than the mark never ends it.
        assert!(badge_within_grace(at, at + ms(5)));
    }

    #[test]
    fn a_sticker_is_waited_for_until_the_wait_is_over() {
        let queued = Instant::now();
        let ms = Duration::from_millis;
        assert!(!sticker_wait_over(queued + ms(1999), queued, START_WAIT));
        assert!(sticker_wait_over(queued + START_WAIT, queued, START_WAIT));
        assert!(!sticker_wait_over(queued, queued + ms(5), START_WAIT));
    }

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
