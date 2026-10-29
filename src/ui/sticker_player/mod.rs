//! Animated stickers and custom emoji on screen: the UI half of the
//! streaming player (`quill::sticker_playback::player` draws the frames).
//!
//! A [`StickerView`] is one place a clip shows: a history row, a panel
//! cell, an emoji in text, an effect. It has its own play head (`HeadId`),
//! so each view plays once, loops or restarts on its own. Views resolve
//! their frame when they paint: the clip is drawn at the painted box times
//! the window's scale factor (sharp on 2x screens), and the frame is the
//! one the head reaches at that moment. Workers draw the next few frames
//! ahead ([`AHEAD`]); a frame that is not ready yet leaves the last one up
//! and counts as late. Nothing is drawn for a view that does not paint
//! (off screen, a hidden panel), so a still chat costs nothing.
//!
//! Frame rate: on a display faster than 60 Hz, with power saving off and
//! the window active, Lottie clips play on a finer grid (up to 120 frames a
//! second, `player::rate_step`); otherwise at the file's own rate. WebM
//! video stickers always keep their own rate. docs/decisions/sticker-player.md.
#![deny(clippy::indexing_slicing)]

mod app;
mod spec;

pub(super) use app::{EmojiSide, PreviewEmoji, ViewAsk, mirrors};
pub(super) use spec::{ClipSpec, HeadId};

use gpui_kit::*;
use quill::sticker_playback::player::{
    AHEAD, ClipKey, ClipState, Engine, PlayHead, PlayMode, Timeline, edge_for,
};
use smallvec::SmallVec;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Bytes the engine keeps for stored frames and open sources together.
/// The three decode-all caches it replaces held up to 176 MB.
pub(super) const BUDGET: usize = 96 * 1024 * 1024;
/// A clip not drawn for this long closes (its frames and canvas go).
const CLOSE_AFTER: Duration = Duration::from_secs(5);
/// Heads not drawn for this long are forgotten (a sticker seen again
/// after that plays as if new).
const HEAD_TTL: Duration = Duration::from_secs(600);
const MAX_HEADS: usize = 4096;
/// A head's wish for frames counts this long after it last drew.
const WANT_TTL: Duration = Duration::from_millis(120);
/// Tick rate while a clip opens.
const OPENING_FPS: u32 = 30;

thread_local! {
    /// The scale factor and refresh rate of the window that last painted a
    /// sticker, for views resolved while rendering (before paint).
    static DISPLAY: Cell<(f32, f64)> = const { Cell::new((2.0, 60.0)) };
}

/// The refresh rate of the window that last painted a sticker.
pub(super) fn display_hz() -> f64 {
    DISPLAY.with(Cell::get).1
}

/// Note the scale factor and refresh rate of the window painting now;
/// called by what paints views (the animation layer, effect overlays).
pub(super) fn note_display(window: &Window, cx: &App) {
    let hz = window
        .refresh_interval(cx)
        .map(|interval| 1.0 / interval.as_secs_f64())
        .filter(|hz| hz.is_finite() && *hz > 0.0)
        .unwrap_or(60.0);
    DISPLAY.with(|display| display.set((window.scale_factor(), hz)));
}

fn trace_stickers() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("QUILL_TRACE_STICKERS").is_some_and(|v| v == "1"))
}

/// Whether views may play on the fine grid now.
fn high_rate(window_active: bool) -> bool {
    window_active
        && display_hz() > 61.0
        && !quill::power_saving::on(quill::power_saving::Flag::Animations)
}

/// What the player did on screen, for `QUILL_TRACE_STICKERS` and tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ScreenStats {
    /// Views drawn.
    pub(super) shown: u64,
    /// Views drawn with an older frame than their head reached.
    pub(super) late: u64,
}

#[derive(Default)]
struct UiClip {
    timeline: Option<Timeline>,
    failed: bool,
    /// Frames drawn and not shown yet, by grid index.
    ready: BTreeMap<u32, Arc<RenderImage>>,
    /// The frame last shown.
    shown: Option<(u32, Arc<RenderImage>)>,
    used: Option<Instant>,
    /// What each head drawing it wants: its frame and the next ones.
    wants: Vec<(HeadId, u32, [Option<u32>; AHEAD], Instant)>,
    /// When its first frame arrived.
    ready_at: Option<Instant>,
}

#[derive(Default)]
struct Inner {
    clips: HashMap<ClipKey, UiClip>,
    heads: HashMap<(HeadId, i32), (PlayHead, Instant)>,
    stats: ScreenStats,
    trimmed: Option<Instant>,
}

/// Frames a test gives a file in place of its own, at an edge.
#[cfg(test)]
pub(super) type Fixture =
    Arc<dyn Fn(u32) -> quill::sticker_playback::player::DecodedFrames + Send + Sync>;

/// The player shared by every view of the app.
pub(crate) struct Player {
    engine: Engine,
    inner: RefCell<Inner>,
    /// The task waking the app when a clip's first frame is ready.
    pump: Cell<bool>,
    #[cfg(test)]
    fixtures: RefCell<HashMap<i32, Fixture>>,
}

impl Player {
    pub(crate) fn new() -> Self {
        Self {
            engine: Engine::new(Engine::default_workers(), BUDGET),
            inner: RefCell::new(Inner::default()),
            pump: Cell::new(false),
            #[cfg(test)]
            fixtures: RefCell::new(HashMap::new()),
        }
    }

    /// Where the frames of `spec` at `edge` come from.
    fn origin(&self, spec: &ClipSpec, edge: u32) -> quill::sticker_playback::player::Origin {
        #[cfg(test)]
        if let Some(fixture) = self
            .fixtures
            .try_borrow()
            .ok()
            .and_then(|fixtures| fixtures.get(&spec.file.0).cloned())
        {
            return quill::sticker_playback::player::Origin::Decoded(Box::new(move |_| {
                Ok(fixture(edge))
            }));
        }
        spec.origin(edge)
    }

    /// Play `fixture`'s frames for `file` (a test without the file).
    #[cfg(test)]
    pub(super) fn set_fixture(&self, file: i32, fixture: Fixture) {
        if let Ok(mut fixtures) = self.fixtures.try_borrow_mut() {
            fixtures.insert(file, fixture);
        }
    }

    #[cfg(test)]
    pub(super) fn has_fixture(&self, file: i32) -> bool {
        self.fixtures
            .try_borrow()
            .is_ok_and(|fixtures| fixtures.contains_key(&file))
    }

    /// Wait (a test) until the workers have nothing left to do, then take
    /// what they drew.
    #[cfg(test)]
    pub(super) fn settle(&self) {
        let until = Instant::now() + Duration::from_secs(10);
        while !self.engine.idle() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(2));
        }
        self.drain();
    }

    /// The image `file` shows or will show first, at any size (a test).
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn any_frame(&self, file: i32) -> Option<Arc<RenderImage>> {
        let inner = self.inner.try_borrow().ok()?;
        inner
            .clips
            .iter()
            .filter(|(key, _)| key.file == file)
            .find_map(|(_, clip)| {
                clip.shown
                    .as_ref()
                    .map(|(_, image)| image.clone())
                    .or_else(|| clip.ready.values().next().cloned())
            })
    }

    /// Whether `key` is open at any size.
    pub(super) fn has_clip_like(&self, key: &ClipKey) -> bool {
        self.inner.try_borrow().is_ok_and(|inner| {
            inner.clips.keys().any(|other| {
                other.file == key.file
                    && other.fitz == key.fitz
                    && other.tint == key.tint
                    && other.mirror == key.mirror
            })
        })
    }

    /// Whether `key` failed to open.
    #[cfg(feature = "demo-capture")]
    pub(super) fn failed_key(&self, key: &ClipKey) -> bool {
        matches!(self.engine.state(key), Some(ClipState::Failed(_)))
    }

    /// Take the frames the workers drew. True when one of them is a clip's
    /// first frame (a view waiting on a still can now animate).
    fn drain(&self) -> bool {
        let rendered = self.engine.take_rendered();
        if rendered.is_empty() {
            return false;
        }
        let mut first = false;
        let mut retired = Vec::new();
        if let Ok(mut inner) = self.inner.try_borrow_mut() {
            for frame in rendered {
                let Some(clip) = inner.clips.get_mut(&frame.key) else {
                    continue;
                };
                let Some(image) = frame_image(frame.key.edge, frame.pixels) else {
                    continue;
                };
                if clip.ready_at.is_none() {
                    clip.ready_at = Some(Instant::now());
                    first = true;
                }
                if let Some(old) = clip.ready.insert(frame.index, image) {
                    retired.push(old);
                }
            }
        }
        super::image_budget::retire_all(retired);
        first
    }

    /// Opens `spec` at `edge` unless it is open; true once a frame of it
    /// (at any size) can show.
    pub(super) fn prepare(&self, spec: &ClipSpec, edge: u32) -> bool {
        self.drain();
        let key = spec.key(edge);
        let Ok(mut inner) = self.inner.try_borrow_mut() else {
            return false;
        };
        let now = Instant::now();
        let clip = inner.clips.entry(key).or_insert_with(|| {
            self.engine.open(key, self.origin(spec, edge));
            UiClip::default()
        });
        clip.used = Some(now);
        if clip.shown.is_some() || !clip.ready.is_empty() {
            return true;
        }
        // Another size of it is on screen (the box changed).
        inner
            .clips
            .iter()
            .any(|(other, clip)| spec.same_file(other) && clip.shown.is_some())
    }

    /// The frame `head` of `spec` shows at `now` drawn `edge` px wide, and
    /// asks for the next ones.
    fn frame(
        &self,
        spec: &ClipSpec,
        head: HeadId,
        mode: PlayMode,
        edge: u32,
        high_rate: bool,
        now: Instant,
    ) -> Option<Arc<RenderImage>> {
        self.drain();
        let key = spec.key(edge);
        let high_rate = high_rate && !self.engine.coarse(&key);
        let mut retired: Vec<Arc<RenderImage>> = Vec::new();
        let shown = {
            let mut guard = self.inner.try_borrow_mut().ok()?;
            let inner = &mut *guard;
            let clip = inner.clips.entry(key).or_insert_with(|| {
                self.engine.open(key, self.origin(spec, edge));
                UiClip::default()
            });
            clip.used = Some(now);
            if clip.timeline.is_none() && !clip.failed {
                match self.engine.state(&key) {
                    Some(ClipState::Ready(info)) => clip.timeline = Some(info.timeline),
                    Some(ClipState::Failed(_)) | None => clip.failed = true,
                    Some(ClipState::Opening) => {}
                }
            }
            match clip.timeline {
                Some(timeline) => {
                    let (head_state, seen) = inner
                        .heads
                        .entry((head, key.file))
                        .or_insert_with(|| (PlayHead::default(), now));
                    *seen = now;
                    let pick = match mode {
                        PlayMode::At(_) => quill::sticker_playback::player::head::pick(
                            Duration::ZERO,
                            mode,
                            timeline,
                            high_rate,
                        ),
                        _ => head_state.advance(now, mode, timeline, high_rate),
                    };
                    pick.and_then(|pick| {
                        let exact = clip
                            .ready
                            .get(&pick.index)
                            .map(|image| (pick.index, image.clone()));
                        if exact.is_some() {
                            head_state.start(now);
                        } else if head_state.started() {
                            inner.stats.late += 1;
                        }
                        // A late head shows an earlier frame: `shown` keeps that
                        // frame's own index, so `ask` never retires it as stale
                        // while it is up.
                        let found = exact
                            .or_else(|| {
                                clip.ready
                                    .range(..pick.index)
                                    .next_back()
                                    .map(|(index, image)| (*index, image.clone()))
                            })
                            .or_else(|| clip.shown.clone());
                        clip.wants.retain(|(h, ..)| *h != head);
                        clip.wants.push((head, pick.index, pick.ahead, now));
                        let (index, image) = found?;
                        if clip
                            .shown
                            .as_ref()
                            .is_none_or(|(_, s)| !Arc::ptr_eq(s, &image))
                            && let Some((_, old)) = clip.shown.replace((index, image.clone()))
                            && !clip.ready.values().any(|r| Arc::ptr_eq(r, &old))
                        {
                            retired.push(old);
                        }
                        inner.stats.shown += 1;
                        Some(image)
                    })
                }
                None if clip.failed => None,
                None => {
                    // Opening: another size's frame meanwhile.
                    inner
                        .clips
                        .iter()
                        .filter(|(other, _)| spec.same_file(other))
                        .find_map(|(_, clip)| clip.shown.as_ref().map(|(_, i)| i.clone()))
                }
            }
        };
        self.ask(&key, now, &mut retired);
        super::image_budget::retire_all(retired);
        self.trim(now);
        shown
    }

    /// Ask the engine for what the heads drawing `key` want next, and let
    /// go of drawn frames none of them needs any more.
    fn ask(&self, key: &ClipKey, now: Instant, retired: &mut Vec<Arc<RenderImage>>) {
        let Ok(mut inner) = self.inner.try_borrow_mut() else {
            return;
        };
        let Some(clip) = inner.clips.get_mut(key) else {
            return;
        };
        clip.wants
            .retain(|(.., at)| now.saturating_duration_since(*at) <= WANT_TTL);
        let mut wanted: SmallVec<[u32; 16]> = SmallVec::new();
        // Each head's own frame first, then the frames ahead in order.
        for (_, index, ..) in &clip.wants {
            wanted.push(*index);
        }
        for slot in 0..AHEAD {
            for (_, _, ahead, _) in &clip.wants {
                if let Some(index) = ahead.get(slot).copied().flatten() {
                    wanted.push(index);
                }
            }
        }
        let keep: SmallVec<[u32; 16]> = wanted.clone();
        let shown = clip.shown.as_ref().map(|(index, _)| *index);
        let stale: Vec<u32> = clip
            .ready
            .keys()
            .copied()
            .filter(|index| !keep.contains(index))
            .collect();
        for index in stale {
            if let Some(image) = clip.ready.remove(&index)
                && shown != Some(index)
            {
                retired.push(image);
            }
        }
        wanted.retain(|index| !clip.ready.contains_key(index));
        let mut seen: SmallVec<[u32; 16]> = SmallVec::new();
        wanted.retain(|index| {
            let fresh = !seen.contains(index);
            seen.push(*index);
            fresh
        });
        drop(inner);
        if !wanted.is_empty() {
            self.engine.want(key, &wanted);
        }
    }

    /// Close clips not drawn lately and forget old heads, once a second.
    fn trim(&self, now: Instant) {
        let Ok(mut inner) = self.inner.try_borrow_mut() else {
            return;
        };
        if inner
            .trimmed
            .is_some_and(|at| now.saturating_duration_since(at) < Duration::from_secs(1))
        {
            return;
        }
        inner.trimmed = Some(now);
        let idle: Vec<ClipKey> = inner
            .clips
            .iter()
            .filter(|(_, clip)| {
                clip.used
                    .is_none_or(|used| now.saturating_duration_since(used) >= CLOSE_AFTER)
            })
            .map(|(key, _)| *key)
            .collect();
        let mut retired = Vec::new();
        for key in idle {
            if let Some(clip) = inner.clips.remove(&key) {
                retired.extend(clip.ready.into_values());
                retired.extend(clip.shown.map(|(_, image)| image));
            }
            self.engine.close(&key);
        }
        inner
            .heads
            .retain(|_, (_, seen)| now.saturating_duration_since(*seen) < HEAD_TTL);
        if inner.heads.len() > MAX_HEADS {
            let mut seen: Vec<Instant> = inner.heads.values().map(|(_, at)| *at).collect();
            seen.sort_unstable();
            if let Some(cut) = seen.get(seen.len() / 2).copied() {
                inner.heads.retain(|_, (_, at)| *at > cut);
            }
        }
        let screen = inner.stats;
        drop(inner);
        super::image_budget::retire_all(retired);
        if trace_stickers() {
            self.trace(screen);
        }
    }

    /// `QUILL_TRACE_STICKERS=1`: once a second, what the player did.
    fn trace(&self, screen: ScreenStats) {
        let stats = self.engine.stats();
        let avg = |time: Duration, count: u64| {
            if count == 0 {
                0.0
            } else {
                time.as_secs_f64() * 1000.0 / count as f64
            }
        };
        eprintln!(
            "stickers: shown {} late {} | drawn {} ({:.3} ms, pack {:.3} ms) unpacked {} ({:.3} ms) | clips {} store {:.1} MB sources {:.1} MB evicted {} | {} Hz",
            screen.shown,
            screen.late,
            stats.drawn,
            avg(stats.draw_time, stats.drawn),
            avg(stats.pack_time, stats.drawn),
            stats.unpacked,
            avg(stats.unpack_time, stats.unpacked),
            stats.clips,
            stats.store_bytes as f64 / 1_048_576.0,
            stats.source_bytes as f64 / 1_048_576.0,
            stats.evicted,
            display_hz().round(),
        );
    }

    /// Close every clip and forget every head (another chat or account:
    /// tdesktop clears the played set when the chat is left).
    pub(super) fn stop(&self) {
        let Ok(mut inner) = self.inner.try_borrow_mut() else {
            return;
        };
        let mut retired = Vec::new();
        for (key, clip) in inner.clips.drain() {
            retired.extend(clip.ready.into_values());
            retired.extend(clip.shown.map(|(_, image)| image));
            self.engine.close(&key);
        }
        inner.heads.clear();
        drop(inner);
        let _ = self.engine.take_rendered();
        super::image_budget::retire_all(retired);
    }

    /// Whether `head` of `file` is known: `Some(started)` while its view is
    /// drawn (and for a while after), `None` once forgotten.
    pub(super) fn head_known(&self, head: HeadId, file: i32) -> Option<bool> {
        let inner = self.inner.try_borrow().ok()?;
        inner
            .heads
            .get(&(head, file))
            .map(|(state, _)| state.started())
    }

    /// How far `head` of `file` played and the clip's length, once its
    /// first frame showed.
    pub(super) fn head_progress(&self, head: HeadId, file: i32) -> Option<(Duration, Duration)> {
        let inner = self.inner.try_borrow().ok()?;
        let (state, _) = inner.heads.get(&(head, file))?;
        if !state.started() {
            return None;
        }
        let total = inner
            .clips
            .iter()
            .filter(|(key, _)| key.file == file)
            .find_map(|(_, clip)| clip.timeline)?
            .duration();
        Some((state.pos(), total))
    }

    /// Play `head` of `file` again from its first frame.
    pub(super) fn restart(&self, head: HeadId, file: i32) {
        if let Ok(mut inner) = self.inner.try_borrow_mut()
            && let Some((state, _)) = inner.heads.get_mut(&(head, file))
        {
            state.restart();
        }
    }

    #[cfg(test)]
    pub(super) fn screen_stats(&self) -> ScreenStats {
        self.inner
            .try_borrow()
            .map(|inner| inner.stats)
            .unwrap_or_default()
    }

    pub(super) fn engine(&self) -> &Engine {
        &self.engine
    }
}

/// `edge` × `edge` straight-alpha BGRA pixels as an image.
fn frame_image(edge: u32, pixels: Vec<u8>) -> Option<Arc<RenderImage>> {
    let rgba = image::RgbaImage::from_raw(edge, edge, pixels)?;
    Some(Arc::new(RenderImage::new(SmallVec::from_buf([
        image::Frame::new(rgba),
    ]))))
}

/// One place a clip shows, with its own play head.
#[derive(Clone)]
pub(super) struct StickerView {
    player: Rc<Player>,
    spec: Rc<ClipSpec>,
    head: HeadId,
    mode: PlayMode,
    /// The edge resolved while rendering (box × last scale factor), for
    /// [`Self::frame_now`].
    edge: u32,
    /// Frames a second at most.
    max_fps: u32,
    /// The window was active when the view was made.
    active: bool,
}

impl StickerView {
    /// The square of `bounds`' longer side, centred on it: frames are
    /// square, and a WebM sticker's square holds its video letterboxed to
    /// the box's shape.
    fn square(bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        let side = bounds.size.width.max(bounds.size.height);
        Bounds::new(
            point(
                bounds.origin.x + (bounds.size.width - side) / 2.,
                bounds.origin.y + (bounds.size.height - side) / 2.,
            ),
            size(side, side),
        )
    }

    /// Paint the frame due now into `bounds`, drawn at their size on
    /// this window.
    pub(super) fn paint(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        let square = Self::square(bounds);
        let edge = edge_for(f32::from(square.size.width) * window.scale_factor());
        let active = window.is_window_active() || super::frame_clock::assume_active();
        let shown = self.player.frame(
            &self.spec,
            self.head,
            self.mode,
            edge,
            high_rate(active),
            Instant::now(),
        );
        if let Some(image) = shown {
            let _ = window.paint_image(square, square, Corners::default(), image, 0, false);
        }
    }

    /// The frame due now at the edge resolved while rendering, for views
    /// shown as an `img` (panels, the composer).
    pub(super) fn frame_now(&self) -> Option<Arc<RenderImage>> {
        self.player.frame(
            &self.spec,
            self.head,
            self.mode,
            self.edge,
            high_rate(self.active),
            Instant::now(),
        )
    }

    /// Frames a second it needs now (0: it rests).
    pub(super) fn fps(&self) -> u32 {
        let Ok(inner) = self.player.inner.try_borrow() else {
            return 0;
        };
        let key = self.spec.key(self.edge);
        // The clip drawn at the painted size, else the one asked for while
        // rendering.
        let clip = inner
            .clips
            .iter()
            .filter(|(other, _)| self.spec.same_file(other))
            .max_by_key(|(_, clip)| clip.used)
            .map(|(_, clip)| clip);
        let Some(timeline) = clip.and_then(|clip| clip.timeline) else {
            return if clip.is_none_or(|clip| clip.failed) {
                0
            } else {
                OPENING_FPS.min(self.max_fps)
            };
        };
        let head = inner
            .heads
            .get(&(self.head, key.file))
            .map(|(head, _)| *head)
            .unwrap_or_default();
        // A clip held to its file's rate still ticks at the display's, so
        // each of its frames stays up for the same number of refreshes.
        let display_high = high_rate(self.active);
        let high = display_high && !self.player.engine.coarse(&key);
        let pick = match self.mode {
            PlayMode::At(_) => quill::sticker_playback::player::head::pick(
                Duration::ZERO,
                self.mode,
                timeline,
                high,
            ),
            _ => {
                let mut head = head;
                head.advance(Instant::now(), self.mode, timeline, high)
            }
        };
        match pick {
            Some(pick) if pick.animating => {
                (timeline.tick_fps(display_high).ceil() as u32).clamp(1, self.max_fps.max(1))
            }
            _ => 0,
        }
    }

    /// When its clip's first frame arrived (at any size), the clip's
    /// readiness for an effect that picks among ready clips.
    pub(super) fn ready_at(&self) -> Option<Instant> {
        let inner = self.player.inner.try_borrow().ok()?;
        inner
            .clips
            .iter()
            .filter(|(key, _)| self.spec.same_file(key))
            .filter_map(|(_, clip)| clip.ready_at)
            .min()
    }

    /// How far its head played and the clip's length, once its first
    /// frame showed.
    pub(super) fn progress(&self) -> Option<(Duration, Duration)> {
        let inner = self.player.inner.try_borrow().ok()?;
        let (head, _) = inner.heads.get(&(self.head, self.spec.file.0))?;
        if !head.started() {
            return None;
        }
        let total = inner
            .clips
            .iter()
            .filter(|(key, _)| self.spec.same_file(key))
            .find_map(|(_, clip)| clip.timeline)?
            .duration();
        Some((head.pos(), total))
    }
}

#[cfg(test)]
mod tests;
