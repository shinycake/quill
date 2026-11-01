//! A sticker flies from the panel into the chat when it is sent (tdesktop
//! `Ui::MessageSendingAnimationController`, `message_sending_animation_controller.cpp`).
//!
//! Choosing a sticker in the panel notes where its cell was and what it
//! showed. When the sent message's sticker is first painted in the open chat
//! it is hidden, and the picture glides from the cell to its place over
//! `itemRevealDuration` (150 ms): the vertical move eases out (quint), the
//! horizontal move eases out a second time on top of it, and the size grows
//! from the cell's to the message's. Afterwards the message shows again.
//!
//! Like tdesktop the effect is skipped when interface animations are off
//! (reduced motion) or "Effects in messages" is on in Battery and
//! animations, and tdesktop starts it for stickers (and GIFs and plain
//! text) only: an emoji sent from the panel goes into the text field, and a
//! message that is only emoji is never animated.
//!
//! The overlay is drawn by the app, over the conversation, so its frames
//! never rebuild the history (the slice stays cached).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::*;
use quill::ids::{ChatId, FileId};
use quill::power_saving;
use quill::state::HistoryMessage;
use quill::telegram::envelope::StickerContent;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// `st::itemRevealDuration`.
const DURATION: Duration = super::motion::REVEAL_DURATION;

/// The flight's length. Tests stretch it so a slow, loaded run cannot see
/// the flight end between two assertions.
fn duration() -> Duration {
    #[cfg(test)]
    if let Some(stretched) = TEST_DURATION.with(std::cell::Cell::get) {
        return stretched;
    }
    DURATION
}

#[cfg(test)]
thread_local! {
    pub(super) static TEST_DURATION: std::cell::Cell<Option<Duration>> =
        const { std::cell::Cell::new(None) };
}
/// A message whose sticker never shows (scrolled away, failed to send)
/// ends the flight after this long.
const WAIT_FOR_TARGET: Duration = Duration::from_secs(2);
/// Cells remembered before the oldest are forgotten.
const MAX_CELLS: usize = 256;
/// A panel cell's padding around its sticker (`p_1`).
const CELL_PADDING: f32 = 4.;
/// Slack when comparing the clock with a message's date.
const DATE_SLACK: i64 = 2;
/// Frame rate asked of the clock while a flight runs or waits.
const TICK_FPS: u32 = 60;

thread_local! {
    /// Where each panel sticker cell was last painted (window coordinates).
    static CELLS: RefCell<HashMap<i32, Bounds<Pixels>>> = RefCell::new(HashMap::new());
    /// What the cell just chosen showed, until the send takes it.
    static SOURCE: RefCell<Option<Source>> = const { RefCell::new(None) };
    static FLY: RefCell<Option<Fly>> = const { RefCell::new(None) };
    /// Where the sent message's sticker was last painted.
    static TARGET: Cell<Option<Bounds<Pixels>>> = const { Cell::new(None) };
    /// The overlay's own top-left in the window, as of its last paint.
    static ORIGIN: Cell<Option<Point<Pixels>>> = const { Cell::new(None) };
}

struct Source {
    file: i32,
    image: ImageSource,
    size: (i32, i32),
}

struct Fly {
    chat_id: i64,
    file: i32,
    /// Unix time the sticker was sent: older messages are not its message.
    since: i64,
    from: Bounds<Pixels>,
    image: ImageSource,
    armed: Instant,
    started: Option<Instant>,
}

/// `anim::easeOutQuint`.
pub(super) fn ease_out_quint(t: f32) -> f32 {
    let rest = 1. - t.clamp(0., 1.);
    1. - rest.powi(5)
}

/// A rectangle as centre and size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Rect {
    pub cx: f32,
    pub cy: f32,
    pub w: f32,
    pub h: f32,
}

/// The picture's rectangle `value` of the way along the flight (`value` is
/// the animation's eased progress, as tdesktop's `animationCallback`
/// receives it): it moves down by `value` and across by `value` eased
/// again, and grows from `from`'s size to `to`'s.
pub(super) fn flight(from: Rect, to: Rect, value: f32) -> Rect {
    let value = value.clamp(0., 1.);
    let across = ease_out_quint(value);
    let mix = |a: f32, b: f32, t: f32| a + (b - a) * t;
    Rect {
        cx: mix(from.cx, to.cx, across),
        cy: mix(from.cy, to.cy, value),
        w: mix(from.w, to.w, value),
        h: mix(from.h, to.h, value),
    }
}

/// `box_w` x `box_h` scaled to fit `(w, h)` and centred in `outer`
/// (`ComputeStickerSize` inside the cell).
pub(super) fn fit(outer: Rect, size: (i32, i32)) -> Rect {
    if size.0 <= 0 || size.1 <= 0 {
        return outer;
    }
    let scale = (outer.w / size.0 as f32).min(outer.h / size.1 as f32);
    Rect {
        w: size.0 as f32 * scale,
        h: size.1 as f32 * scale,
        ..outer
    }
}

fn rect_of(bounds: Bounds<Pixels>) -> Rect {
    Rect {
        cx: f32::from(bounds.origin.x + bounds.size.width / 2.),
        cy: f32::from(bounds.origin.y + bounds.size.height / 2.),
        w: f32::from(bounds.size.width),
        h: f32::from(bounds.size.height),
    }
}

/// Whether a message is the one a sticker just sent from the panel became.
pub(super) fn is_sent_sticker(
    (chat_id, file): (i64, i32),
    since: i64,
    message: (i64, i32, bool, bool, i64),
) -> bool {
    let (message_chat, message_file, outgoing, pending, date) = message;
    outgoing
        && message_chat == chat_id
        && message_file == file
        && (pending || date >= since - DATE_SLACK)
}

fn matches_fly(fly: &Fly, message: &HistoryMessage, sticker: &StickerContent) -> bool {
    is_sent_sticker(
        (fly.chat_id, fly.file),
        fly.since,
        (
            message.chat_id.0,
            sticker.file_id.0,
            message.is_outgoing,
            message.pending,
            i64::from(message.date),
        ),
    )
}

/// Notes where a panel sticker cell is painted.
pub(super) fn cell_probe(file: FileId) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let visible = bounds.intersect(&window.content_mask().bounds);
            if visible.size.width <= Pixels::ZERO || visible.size.height <= Pixels::ZERO {
                return;
            }
            CELLS.with(|cells| {
                let mut cells = cells.borrow_mut();
                if cells.len() >= MAX_CELLS && !cells.contains_key(&file.0) {
                    cells.clear();
                }
                cells.insert(file.0, bounds);
            });
        },
    )
    .absolute()
    .inset_0()
    .size_full()
}

/// The picture of the cell being chosen, for the send that follows.
pub(super) fn note_source(file: FileId, image: Option<ImageSource>, size: (i32, i32)) {
    SOURCE.with(|source| {
        *source.borrow_mut() = image.map(|image| Source {
            file: file.0,
            image,
            size,
        });
    });
}

/// The row of a message in the chat: the sticker is hidden while it flies
/// in, and its place is noted. Every other message is returned as it is.
pub(super) fn wrap(
    message: &HistoryMessage,
    sticker: &StickerContent,
    attachment: AnyElement,
) -> AnyElement {
    let key = (message.chat_id.0, message.id.0);
    let hidden = FLY.with(|fly| {
        fly.borrow()
            .as_ref()
            .is_some_and(|fly| matches_fly(fly, message, sticker))
    });
    if !hidden {
        return attachment;
    }
    let (w, h) = super::message_media::sticker_box(sticker.width, sticker.height);
    div()
        .relative()
        .flex_none()
        .w(w)
        // The attachment is `h` tall below an 8 px margin.
        .child(div().mt_2().w(w).h(h))
        .child(
            canvas(
                |_, _, _| {},
                move |block, _, window, _| {
                    let bounds = Bounds::new(
                        point(block.left(), block.bottom() - h),
                        size(block.size.width, h),
                    );
                    let visible = bounds.intersect(&window.content_mask().bounds);
                    if visible.size.width > Pixels::ZERO && visible.size.height > Pixels::ZERO {
                        TARGET.with(|target| target.set(Some(bounds)));
                    }
                },
            )
            .absolute()
            .inset_0()
            .size_full(),
        )
        .id(("sticker-flying-in", key.1 as u64))
        .into_any_element()
}

impl QuillApp {
    /// A sticker was sent from the panel: start its flight, unless motion
    /// is reduced or message effects are off.
    pub(super) fn arm_send_fly(&mut self, chat_id: ChatId, file: FileId, cx: &mut Context<Self>) {
        let source = SOURCE.with(|source| source.borrow_mut().take());
        let Some(source) = source.filter(|source| source.file == file.0) else {
            return;
        };
        if cx.reduce_motion() || power_saving::on(power_saving::Flag::ChatEffects) {
            return;
        }
        let Some(cell) = CELLS.with(|cells| cells.borrow().get(&file.0).copied()) else {
            return;
        };
        let inner = {
            let cell = rect_of(cell);
            Rect {
                w: (cell.w - 2. * CELL_PADDING).max(1.),
                h: (cell.h - 2. * CELL_PADDING).max(1.),
                ..cell
            }
        };
        let fitted = fit(inner, source.size);
        let from = Bounds::new(
            point(px(fitted.cx - fitted.w / 2.), px(fitted.cy - fitted.h / 2.)),
            size(px(fitted.w), px(fitted.h)),
        );
        let since = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs() as i64);
        TARGET.with(|target| target.set(None));
        FLY.with(|fly| {
            *fly.borrow_mut() = Some(Fly {
                chat_id: chat_id.0,
                file: file.0,
                since,
                from,
                image: source.image,
                armed: Instant::now(),
                started: None,
            });
        });
        self.notify_conversation(cx);
    }

    /// The flying picture, over the conversation; `None` when nothing
    /// flies. Ends the flight (and shows the message) when it lands.
    pub(super) fn send_fly_overlay(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let now = Instant::now();
        let step = FLY.with(|fly| {
            let mut fly = fly.borrow_mut();
            let current = fly.as_mut()?;
            if current.started.is_none() && TARGET.with(Cell::get).is_some() {
                current.started = Some(now);
            }
            let ended = match current.started {
                Some(started) => now.duration_since(started) >= duration(),
                None => now.duration_since(current.armed) >= WAIT_FOR_TARGET,
            };
            if ended {
                *fly = None;
                return Some(None);
            }
            Some(Some((current.from, current.image.clone(), current.started)))
        });
        let step = step?;
        let Some((from, image, started)) = step else {
            // Done: show the real sticker.
            TARGET.with(|target| target.set(None));
            self.notify_conversation(cx);
            return None;
        };
        self.request_app_tick(TICK_FPS, cx);
        let origin = ORIGIN.with(Cell::get);
        let positioned = started.zip(TARGET.with(Cell::get)).zip(origin);
        let picture = positioned.map(|((started, target), origin)| {
            let progress = super::motion::progress(now.duration_since(started), duration());
            let at = flight(rect_of(from), rect_of(target), ease_out_quint(progress));
            div()
                .absolute()
                .left(px(at.cx - at.w / 2.) - origin.x)
                .top(px(at.cy - at.h / 2.) - origin.y)
                .w(px(at.w))
                .h(px(at.h))
                .child(img(image).size_full().object_fit(ObjectFit::Contain))
        });
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .child(
                    canvas(
                        |bounds, _, _| ORIGIN.with(|origin| origin.set(Some(bounds.origin))),
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset_0()
                    .size_full(),
                )
                .children(picture)
                .into_any_element(),
        )
    }
}

/// Whether a flight is armed, and whether it has started moving.
#[cfg(test)]
#[cfg(feature = "demo-capture")]
pub(super) fn flight_state() -> Option<bool> {
    FLY.with(|fly| fly.borrow().as_ref().map(|fly| fly.started.is_some()))
}

/// Forget any flight and what was noted (tests share a thread).
#[cfg(test)]
#[cfg(feature = "demo-capture")]
pub(super) fn reset() {
    FLY.with(|fly| *fly.borrow_mut() = None);
    SOURCE.with(|source| *source.borrow_mut() = None);
    TARGET.with(|target| target.set(None));
}

#[cfg(test)]
mod tests {
    use super::{Rect, ease_out_quint, fit, flight, is_sent_sticker};

    fn rect(cx: f32, cy: f32, w: f32, h: f32) -> Rect {
        Rect { cx, cy, w, h }
    }

    #[test]
    fn quint_easing_runs_from_zero_to_one_and_leads() {
        assert_eq!(ease_out_quint(0.), 0.);
        assert_eq!(ease_out_quint(1.), 1.);
        assert!(ease_out_quint(0.2) > 0.6, "front-loaded");
        assert_eq!(ease_out_quint(2.), 1.);
    }

    #[test]
    fn the_flight_starts_at_the_cell_and_lands_on_the_message() {
        let cell = rect(100., 400., 64., 64.);
        let message = rect(500., 300., 160., 160.);
        assert_eq!(flight(cell, message, 0.), cell);
        assert_eq!(flight(cell, message, 1.), message);
        let middle = flight(cell, message, 0.5);
        // Across is eased on top of the eased progress: ahead of the
        // vertical move, which is linear in that progress.
        let across = (middle.cx - cell.cx) / (message.cx - cell.cx);
        let down = (middle.cy - cell.cy) / (message.cy - cell.cy);
        assert!((down - 0.5).abs() < 1e-5);
        assert!(across > down);
        assert!((middle.w - 112.).abs() < 1e-4, "size grows linearly");
    }

    #[test]
    fn a_sticker_is_fitted_into_its_cell_keeping_its_shape() {
        let cell = rect(50., 50., 64., 64.);
        let wide = fit(cell, (512, 256));
        assert_eq!((wide.w, wide.h), (64., 32.));
        assert_eq!((wide.cx, wide.cy), (50., 50.));
        assert_eq!(fit(cell, (0, 0)), cell);
    }

    #[test]
    fn only_a_fresh_outgoing_sticker_of_that_file_is_the_message() {
        let fly = ((7, 41), 1_000);
        // Pending (just handed to TDLib).
        assert!(is_sent_sticker(fly.0, fly.1, (7, 41, true, true, 0)));
        // Sent, dated just after the click.
        assert!(is_sent_sticker(fly.0, fly.1, (7, 41, true, false, 1_000)));
        // The same sticker sent earlier, another file, another chat, an
        // incoming one.
        assert!(!is_sent_sticker(fly.0, fly.1, (7, 41, true, false, 900)));
        assert!(!is_sent_sticker(fly.0, fly.1, (7, 42, true, false, 1_001)));
        assert!(!is_sent_sticker(fly.0, fly.1, (8, 41, true, false, 1_001)));
        assert!(!is_sent_sticker(fly.0, fly.1, (7, 41, false, false, 1_001)));
    }
}
