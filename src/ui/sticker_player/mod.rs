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
mod view;

#[cfg(test)]
pub(super) use app::sidebar_emoji_head;
pub(super) use app::{EmojiSide, PreviewEmoji, ViewAsk, mirrors, panel_sticker_head};
pub(super) use spec::{ClipSpec, HeadId, Part};
pub(super) use view::StickerView;

use super::image_budget::PaintStamp;
use futures_channel::oneshot;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::sticker_playback::player::{
    AHEAD, ClipKey, ClipState, Engine, PlayHead, PlayMode, Timeline, edge_for,
};
use smallvec::SmallVec;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Bytes the engine keeps for stored frames and open sources together.
/// The three decode-all caches it replaces held up to 176 MB.
pub(super) const BUDGET: usize = 96 * 1024 * 1024;
/// A clip no window has shown for this long closes (its frames and
/// canvas go).
const CLOSE_AFTER: Duration = Duration::from_secs(5);
/// The closer looks again this long after a window drew, and this long
/// past a clip's due time: a burst of frames, or clips due close
/// together, cost one look.
const CLOSE_SLACK: Duration = Duration::from_secs(1);
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
    /// Where views last drew it: one stamp per window and slice.
    drawn: SmallVec<[PaintStamp; 2]>,
    /// When the closer last found it on screen.
    seen: Option<Instant>,
    /// What each head drawing it wants.
    wants: Vec<Want>,
    /// When its first frame arrived.
    ready_at: Option<Instant>,
    /// Where views asked for it since it opened.
    places: Places,
}

/// What a head drawing a clip wants.
struct Want {
    head: HeadId,
    index: u32,
    ahead: [Option<u32>; AHEAD],
    at: Instant,
    /// It rests on `index`, and its window draws it only when something else
    /// changes: the frame stays for as long as the head does.
    rests: bool,
}

/// Where a view is in a clip.
struct Head {
    state: PlayHead,
    /// When a view last drew it.
    seen: Instant,
    /// Where views drew it.
    places: Places,
}

/// Where a view shows: a window (`None`: the main window, else the chat
/// window of that chat) and the part of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Place {
    window: Option<ChatId>,
    part: Part,
}

impl Place {
    /// Where the view of `head` in `window` shows.
    pub(super) fn of(head: HeadId, window: Option<ChatId>) -> Self {
        Self {
            window,
            part: head.part(),
        }
    }
}

type Places = SmallVec<[Place; 2]>;

/// Note that a view at `place` used a clip or a head.
fn note_place(places: &mut Places, place: Place) {
    if !places.contains(&place) {
        places.push(place);
    }
}

/// What a window lets go of ([`Player::leave`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Leaving {
    /// Its chat: another chat opens there, or the chat moves to a window
    /// of its own. The rest of the window stays as it is.
    Chat,
    /// All of it: the window closes.
    Window,
}

impl Leaving {
    /// Whether `window` letting go of this loses what views at `place` use.
    fn loses(self, window: Option<ChatId>, place: Place) -> bool {
        place.window == window && (self == Leaving::Window || place.part == Part::Chat)
    }
}

#[derive(Default)]
struct Inner {
    clips: HashMap<ClipKey, UiClip>,
    heads: HashMap<(HeadId, i32), Head>,
    stats: ScreenStats,
    trimmed: Option<Instant>,
}

/// What [`Player::close_stale`] did.
pub(super) struct Closed {
    /// The frames the clips it closed held, each once.
    pub(super) frames: Vec<Arc<RenderImage>>,
    /// How many clips it closed.
    pub(super) clips: usize,
    /// How long until the next clip off screen is due, if any.
    pub(super) next: Option<Duration>,
    /// Clips are still open.
    pub(super) open: bool,
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
    /// The task closing clips once no window shows them (`close_stale`).
    closer: Cell<bool>,
    /// Wakes the closer at the next frame of any window.
    next_frame: RefCell<Option<oneshot::Sender<()>>>,
    #[cfg(test)]
    fixtures: RefCell<HashMap<i32, Fixture>>,
    /// How far a test moved the clock on.
    #[cfg(test)]
    skew: Cell<Duration>,
    /// Where a test stopped the clock (`freeze`).
    #[cfg(test)]
    frozen: Cell<Option<Instant>>,
    /// How many times `close_stale` looked (a test).
    #[cfg(test)]
    looks: Cell<usize>,
}

impl Player {
    pub(crate) fn new() -> Self {
        Self {
            engine: Engine::new(Engine::default_workers(), BUDGET),
            inner: RefCell::new(Inner::default()),
            pump: Cell::new(false),
            closer: Cell::new(false),
            next_frame: RefCell::new(None),
            #[cfg(test)]
            fixtures: RefCell::new(HashMap::new()),
            #[cfg(test)]
            skew: Cell::new(Duration::ZERO),
            #[cfg(test)]
            frozen: Cell::new(None),
            #[cfg(test)]
            looks: Cell::new(0),
        }
    }

    /// The time views draw at and the closer judges by.
    pub(super) fn now(&self) -> Instant {
        #[cfg(test)]
        return self.frozen.get().unwrap_or_else(Instant::now) + self.skew.get();
        #[cfg(not(test))]
        Instant::now()
    }

    /// Move [`Self::now`] on (a test, with the executor's clock).
    #[cfg(test)]
    pub(super) fn skip(&self, by: Duration) {
        self.skew.set(self.skew.get() + by);
    }

    /// Stop [`Self::now`] where it is: from then on only [`Self::skip`]
    /// moves it, so a test knows how far a head played.
    #[cfg(test)]
    pub(super) fn freeze(&self) {
        self.frozen.set(Some(Instant::now()));
    }

    /// How many times the closer looked (a test).
    #[cfg(test)]
    pub(super) fn looks(&self) -> usize {
        self.looks.get()
    }

    /// When each open clip's first frame arrived, in order (a test): a clip
    /// closed and opened again has a later one, or none yet.
    #[cfg(test)]
    pub(super) fn first_frames(&self) -> Vec<Option<Instant>> {
        self.first_frames_of(None)
    }

    /// [`Self::first_frames`] of `file`'s clips only (`None`: every clip).
    #[cfg(test)]
    pub(super) fn first_frames_of(&self, file: Option<i32>) -> Vec<Option<Instant>> {
        let mut first: Vec<Option<Instant>> = self
            .inner
            .try_borrow()
            .map(|inner| {
                inner
                    .clips
                    .iter()
                    .filter(|(key, _)| file.is_none_or(|file| key.file == file))
                    .map(|(_, clip)| clip.ready_at)
                    .collect()
            })
            .unwrap_or_default();
        first.sort_unstable();
        first
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

    /// How many clips of `file` are open, at any size (a test).
    #[cfg(test)]
    pub(super) fn clips_of(&self, file: i32) -> usize {
        self.inner.try_borrow().map_or(0, |inner| {
            inner.clips.keys().filter(|key| key.file == file).count()
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

    /// Opens `spec` at `edge` for a view at `place` unless it is open; true
    /// once a frame of it (at any size) can show.
    pub(super) fn prepare(&self, spec: &ClipSpec, edge: u32, place: Place) -> bool {
        self.drain();
        let key = spec.key(edge);
        let Ok(mut inner) = self.inner.try_borrow_mut() else {
            return false;
        };
        let now = self.now();
        let clip = inner.clips.entry(key).or_insert_with(|| {
            self.engine.open(key, self.origin(spec, edge));
            UiClip::default()
        });
        clip.used = Some(now);
        note_place(&mut clip.places, place);
        if clip.shown.is_some() || !clip.ready.is_empty() {
            return true;
        }
        // Another size of it is on screen (the box changed).
        inner
            .clips
            .iter()
            .any(|(other, clip)| spec.same_file(other) && clip.shown.is_some())
    }

    /// The frame `head` of `spec` shows at `now` drawn `edge` px wide in
    /// `window` (`None`: the main window), and asks for the next ones.
    #[allow(clippy::too_many_arguments)]
    fn frame(
        &self,
        spec: &ClipSpec,
        head: HeadId,
        mode: PlayMode,
        edge: u32,
        high_rate: bool,
        window: Option<ChatId>,
        now: Instant,
    ) -> Option<Arc<RenderImage>> {
        self.drain();
        let key = spec.key(edge);
        let high_rate = high_rate && !self.engine.coarse(&key);
        let place = Place::of(head, window);
        let mut retired: Vec<Arc<RenderImage>> = Vec::new();
        let shown = {
            let mut guard = self.inner.try_borrow_mut().ok()?;
            let inner = &mut *guard;
            let drawn = super::image_budget::paint_stamp_in(window);
            let clip = inner.clips.entry(key).or_insert_with(|| {
                self.engine.open(key, self.origin(spec, edge));
                UiClip::default()
            });
            clip.used = Some(now);
            note_drawn(&mut clip.drawn, drawn);
            note_place(&mut clip.places, place);
            if clip.timeline.is_none() && !clip.failed {
                match self.engine.state(&key) {
                    Some(ClipState::Ready(info)) => clip.timeline = Some(info.timeline),
                    Some(ClipState::Failed(_)) | None => clip.failed = true,
                    Some(ClipState::Opening) => {}
                }
            }
            match clip.timeline {
                Some(timeline) => {
                    let entry = inner.heads.entry((head, key.file)).or_insert_with(|| Head {
                        state: PlayHead::default(),
                        seen: now,
                        places: Places::new(),
                    });
                    entry.seen = now;
                    note_place(&mut entry.places, place);
                    let head_state = &mut entry.state;
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
                        clip.wants.retain(|want| want.head != head);
                        clip.wants.push(Want {
                            head,
                            index: pick.index,
                            ahead: pick.ahead,
                            at: now,
                            rests: !pick.animating,
                        });
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
                    // Opening: another size's frame meanwhile, on screen
                    // as that clip's.
                    let other = inner
                        .clips
                        .iter_mut()
                        .filter(|(other, _)| spec.same_file(other))
                        .find_map(|(_, clip)| clip.shown.is_some().then_some(clip));
                    other.and_then(|other| {
                        note_drawn(&mut other.drawn, drawn);
                        note_place(&mut other.places, place);
                        other.shown.as_ref().map(|(_, image)| image.clone())
                    })
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
        let Ok(mut guard) = self.inner.try_borrow_mut() else {
            return;
        };
        let inner = &mut *guard;
        let Some(clip) = inner.clips.get_mut(key) else {
            return;
        };
        let heads = &inner.heads;
        clip.wants.retain(|want| {
            now.saturating_duration_since(want.at) <= WANT_TTL
                || want.rests && heads.contains_key(&(want.head, key.file))
        });
        let mut wanted: SmallVec<[u32; 16]> = SmallVec::new();
        // Each head's own frame first, then the frames ahead in order.
        for want in &clip.wants {
            wanted.push(want.index);
        }
        for slot in 0..AHEAD {
            for want in &clip.wants {
                if let Some(index) = want.ahead.get(slot).copied().flatten() {
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
        drop(guard);
        if !wanted.is_empty() {
            self.engine.want(key, &wanted);
        }
    }

    /// Forget heads not drawn lately, once a second. Clips close between
    /// frames (`close_stale`): in the middle of one, a view at rest that
    /// the frame has not painted yet would look gone.
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
        inner
            .heads
            .retain(|_, head| now.saturating_duration_since(head.seen) < HEAD_TTL);
        if inner.heads.len() > MAX_HEADS {
            let mut seen: Vec<Instant> = inner.heads.values().map(|head| head.seen).collect();
            seen.sort_unstable();
            if let Some(cut) = seen.get(seen.len() / 2).copied() {
                inner.heads.retain(|_, head| head.seen > cut);
            }
        }
        let screen = inner.stats;
        drop(inner);
        if trace_stickers() {
            self.trace(screen);
        }
    }

    /// Close the clips no window has shown for [`CLOSE_AFTER`]. Call
    /// between frames: a clip counts as shown while the latest frame of a
    /// window drew it, or replays a slice render that did (a view at rest
    /// is not drawn again while nothing changes, and closing its clip
    /// would show the still at the next draw). No window shows what the
    /// closed clips held, so their frames can leave the atlas now.
    pub(super) fn close_stale(&self, now: Instant) -> Closed {
        #[cfg(test)]
        self.looks.set(self.looks.get() + 1);
        let Ok(mut guard) = self.inner.try_borrow_mut() else {
            return Closed {
                frames: Vec::new(),
                clips: 0,
                next: Some(CLOSE_AFTER),
                open: true,
            };
        };
        let inner = &mut *guard;
        let mut next: Option<Duration> = None;
        let mut stale = Vec::new();
        for (key, clip) in &mut inner.clips {
            clip.drawn
                .retain(|drawn| super::image_budget::shows_now(*drawn));
            if !clip.drawn.is_empty() {
                clip.seen = Some(now);
                continue;
            }
            let idle = clip
                .used
                .max(clip.seen)
                .map_or(CLOSE_AFTER, |at| now.saturating_duration_since(at));
            match CLOSE_AFTER.checked_sub(idle).filter(|left| !left.is_zero()) {
                Some(left) => next = Some(next.map_or(left, |next| next.min(left))),
                None => stale.push(*key),
            }
        }
        let mut frames: Vec<Arc<RenderImage>> = Vec::new();
        for key in &stale {
            if let Some(clip) = inner.clips.remove(key) {
                for image in clip.ready.into_values().chain(clip.shown.map(|(_, i)| i)) {
                    if !frames.iter().any(|kept| Arc::ptr_eq(kept, &image)) {
                        frames.push(image);
                    }
                }
            }
            self.engine.close(key);
        }
        Closed {
            frames,
            clips: stale.len(),
            next,
            open: !inner.clips.is_empty(),
        }
    }

    /// A window starts a frame, before anything in it renders.
    pub(super) fn frame_starts(&self) {
        self.screen_changes();
    }

    /// A chat window closed: what it showed is gone.
    pub(super) fn window_closed(&self, chat: ChatId) {
        super::image_budget::forget_window(chat);
        self.screen_changes();
    }

    /// What is on screen may change now. If the closer waits for that,
    /// the clips on screen stayed there until now: their wait to close
    /// starts here, not at the closer's last look (minutes ago, for a
    /// view at rest).
    fn screen_changes(&self) {
        let Some(wake) = self
            .next_frame
            .try_borrow_mut()
            .ok()
            .and_then(|mut next| next.take())
        else {
            return;
        };
        let now = self.now();
        if let Ok(mut inner) = self.inner.try_borrow_mut() {
            for clip in inner.clips.values_mut() {
                if clip
                    .drawn
                    .iter()
                    .any(|drawn| super::image_budget::shows_now(*drawn))
                {
                    clip.seen = Some(now);
                }
            }
        }
        let _ = wake.send(());
    }

    /// Fires at the next frame of any window, or when one closes.
    fn next_frame(&self) -> oneshot::Receiver<()> {
        let (wake, next) = oneshot::channel();
        if let Ok(mut slot) = self.next_frame.try_borrow_mut() {
            *slot = Some(wake);
        }
        next
    }

    /// Whether any clip is open.
    fn has_clips(&self) -> bool {
        self.inner
            .try_borrow()
            .is_ok_and(|inner| !inner.clips.is_empty())
    }

    /// `QUILL_TRACE_STICKERS=1`: the closer closed `clips`; what is left.
    fn trace_closed(&self, clips: usize) {
        if clips == 0 || !trace_stickers() {
            return;
        }
        eprintln!("stickers: closed {clips} clips no window shows");
        if let Ok(inner) = self.inner.try_borrow() {
            let screen = inner.stats;
            drop(inner);
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

    /// `window` (`None`: the main window) let go of its chat, or of all
    /// of it (`what`): another chat opened there, or the window closed.
    /// The heads only views there drew are forgotten, so the chat plays
    /// from the start when it shows again (tdesktop clears the played set
    /// when the chat is left), and the clips only views there asked for
    /// close. What views elsewhere use plays on untouched: in another
    /// window, or in the rest of this one when only its chat goes (the
    /// chat list keeps its emoji where they are, on their frames).
    /// Returns the frames of the clips it closed, each once.
    pub(super) fn leave(&self, window: Option<ChatId>, what: Leaving) -> Vec<Arc<RenderImage>> {
        let Ok(mut guard) = self.inner.try_borrow_mut() else {
            return Vec::new();
        };
        let inner = &mut *guard;
        inner.heads.retain(|_, head| {
            head.places.retain(|place| !what.loses(window, *place));
            !head.places.is_empty()
        });
        let mut frames: Vec<Arc<RenderImage>> = Vec::new();
        let mut closed = Vec::new();
        inner.clips.retain(|key, clip| {
            clip.places.retain(|place| !what.loses(window, *place));
            if !clip.places.is_empty() {
                return true;
            }
            let shown = clip.shown.take().map(|(_, image)| image);
            for image in std::mem::take(&mut clip.ready).into_values().chain(shown) {
                if !frames.iter().any(|kept| Arc::ptr_eq(kept, &image)) {
                    frames.push(image);
                }
            }
            closed.push(*key);
            false
        });
        drop(guard);
        for key in &closed {
            self.engine.close(key);
        }
        frames
    }

    /// Close every clip and forget every head, in every window (another
    /// account, the loop setting changed, the app goes).
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
            .map(|head| head.state.started())
    }

    /// How far `head` of `file` played and the clip's length, once its
    /// first frame showed.
    pub(super) fn head_progress(&self, head: HeadId, file: i32) -> Option<(Duration, Duration)> {
        let inner = self.inner.try_borrow().ok()?;
        let state = &inner.heads.get(&(head, file))?.state;
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
            && let Some(head) = inner.heads.get_mut(&(head, file))
        {
            head.state.restart();
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

/// Note that a view drew a clip at `drawn`, over its last stamp there.
fn note_drawn(stamps: &mut SmallVec<[PaintStamp; 2]>, drawn: PaintStamp) {
    match stamps.iter_mut().find(|stamp| stamp.same_place(drawn)) {
        Some(stamp) => *stamp = drawn,
        None => stamps.push(drawn),
    }
}

/// `edge` × `edge` straight-alpha BGRA pixels as an image.
fn frame_image(edge: u32, pixels: Vec<u8>) -> Option<Arc<RenderImage>> {
    let rgba = image::RgbaImage::from_raw(edge, edge, pixels)?;
    Some(Arc::new(RenderImage::new(SmallVec::from_buf([
        image::Frame::new(rgba),
    ]))))
}

#[cfg(test)]
mod tests;
