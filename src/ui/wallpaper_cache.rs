//! Rendered wallpapers: one image per (wallpaper, pixel size), made on a
//! worker thread and painted as a single quad (tdesktop `ChatTheme::
//! backgroundState` / `CacheBackground`).
//!
//! - A canvas asks for its wallpaper at its device-pixel size. A ready
//!   image paints 1:1; otherwise the image that area showed last paints
//!   stretched (tdesktop does the same while it re-caches after a resize)
//!   and the frame clock is asked for another frame until the new one
//!   arrives.
//! - Each area is a *slot* (the history of one window, the preview, a
//!   thumbnail). A new request for a slot replaces its queued one, and a
//!   resize waits [`SETTLE`] for the size to stop changing, so a window
//!   drag renders once at the end, not on every frame.
//! - Images stay under a byte budget; evicted ones are retired from the
//!   sprite atlas through `image_budget`. Painting a ready image does no
//!   pixel work and allocates no pixels.
//! - Once Quill is idle, [`release_unshown`] drops the images no area
//!   shows any more: each window size a resize went through (a full
//!   history picture each), a wallpaper since turned or changed, a closed
//!   dialog's thumbnails, a closed chat window's history.
#![deny(clippy::indexing_slicing)]

use super::image_budget;
use super::wallpaper::Wallpaper;
use super::wallpaper_source::{Rendered, Sources};
use gpui_kit::*;
use quill::locks::LockExt;
use smallvec::SmallVec;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// How long a resized area's size must hold before it re-renders.
const SETTLE: Duration = Duration::from_millis(150);
/// Rendered images kept (a 1200 x 900 area at 2x is about 17 MB).
const BUDGET_BYTES: usize = 64 << 20;
/// Finished results nobody picked up are dropped past this many.
const DONE_CAP: usize = 48;
/// A turned freeform gradient crossfades in (tdesktop
/// `kBackgroundFadeDuration`).
const FADE: Duration = Duration::from_millis(200);

/// Which area a request is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Slot {
    /// The history of one window (the canvas adds the window id). The
    /// entity re-renders when a crossfade starts.
    History(EntityId),
    /// The wallpaper preview in a dialog.
    Preview,
    /// A gallery thumbnail: fixed size, never replaced or delayed.
    Thumb,
}

/// The cache key of `wallpaper` at `width` x `height` pixels.
pub(super) fn image_key(wallpaper: &Wallpaper, width: u32, height: u32) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    wallpaper.hash(&mut hasher);
    width.hash(&mut hasher);
    height.hash(&mut hasher);
    hasher.finish()
}

struct Job {
    key: u64,
    slot: u64,
    wallpaper: Wallpaper,
    width: u32,
    height: u32,
    /// Not before this instant (a resize settling).
    not_before: Instant,
}

/// Requests and results shared with the worker thread.
#[derive(Default)]
struct Board {
    queue: Vec<Job>,
    /// Keys still wanted (queued or rendering); a result for a key that
    /// is no longer wanted is dropped.
    wanted: HashSet<u64>,
    running: Option<(u64, u64)>,
    /// Finished results by key, with their slot, until a lookup takes
    /// them.
    done: HashMap<u64, (u64, Option<Rendered>)>,
}

impl Board {
    /// Queue `job`, replacing the slot's older request and any result for
    /// it nobody took: a size the window was at for one frame when its
    /// render finished (a full history picture) would otherwise wait in
    /// `done` until `DONE_CAP` clears it.
    fn request(&mut self, job: Job, replaces: bool) {
        if self.wanted.contains(&job.key) || self.done.contains_key(&job.key) {
            return;
        }
        if replaces {
            let slot = job.slot;
            let mut dropped = Vec::new();
            self.queue.retain(|j| {
                let keep = j.slot != slot;
                if !keep {
                    dropped.push(j.key);
                }
                keep
            });
            if let Some((running_slot, running_key)) = self.running
                && running_slot == slot
            {
                dropped.push(running_key);
            }
            for key in dropped {
                self.wanted.remove(&key);
            }
            self.done.retain(|_, (done_slot, _)| *done_slot != slot);
        }
        self.wanted.insert(job.key);
        self.queue.push(job);
    }

    /// The next job that may start now, or how long until one may.
    fn next(&mut self, now: Instant) -> Result<Job, Option<Duration>> {
        match self.queue.iter().position(|j| j.not_before <= now) {
            Some(at) => {
                let job = self.queue.remove(at);
                self.running = Some((job.slot, job.key));
                Ok(job)
            }
            None => Err(self
                .queue
                .iter()
                .map(|j| j.not_before.saturating_duration_since(now))
                .min()),
        }
    }

    fn finish(&mut self, slot: u64, key: u64, result: Option<Rendered>) {
        self.running = None;
        if self.wanted.remove(&key) {
            if self.done.len() >= DONE_CAP {
                self.done.clear();
            }
            self.done.insert(key, (slot, result));
        }
    }
}

struct Shared {
    board: Mutex<Board>,
    wake: Condvar,
    /// The worker thread started; if it could not, images render on the
    /// calling thread instead of never arriving.
    threaded: bool,
}

fn shared() -> &'static Shared {
    static SHARED: OnceLock<Shared> = OnceLock::new();
    SHARED.get_or_init(|| {
        let threaded = quill::thread_guard::spawn_guarded("quill-wallpaper", worker).is_ok();
        Shared {
            board: Mutex::new(Board::default()),
            wake: Condvar::new(),
            threaded,
        }
    })
}

thread_local! {
    /// Sources for the no-thread fallback.
    static INLINE_SOURCES: RefCell<Sources> = RefCell::new(Sources::default());
}

fn worker() {
    let shared = shared();
    let mut sources = Sources::default();
    loop {
        let job = {
            let mut board = shared.board.lock_or_recover();
            loop {
                match board.next(Instant::now()) {
                    Ok(job) => break job,
                    Err(Some(wait)) => {
                        board = shared
                            .wake
                            .wait_timeout(board, wait)
                            .unwrap_or_else(|e| e.into_inner())
                            .0;
                    }
                    Err(None) => {
                        board = shared.wake.wait(board).unwrap_or_else(|e| e.into_inner());
                    }
                }
            }
        };
        let result = sources.render(&job.wallpaper, job.width, job.height);
        shared
            .board
            .lock_or_recover()
            .finish(job.slot, job.key, result);
    }
}

/// A rendered wallpaper ready to paint.
#[derive(Clone)]
pub(super) struct Composite {
    image: Arc<RenderImage>,
    /// The area it was made for and where the image sits in it, in
    /// device pixels.
    area: (u32, u32),
    at: (i32, i32),
    size: (u32, u32),
}

impl Composite {
    /// Where the image goes when the area is `bounds` (stretched when the
    /// area has since changed size).
    fn image_bounds(&self, bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        let sx = bounds.size.width / self.area.0.max(1) as f32;
        let sy = bounds.size.height / self.area.1.max(1) as f32;
        Bounds::new(
            point(
                bounds.origin.x + sx * self.at.0 as f32,
                bounds.origin.y + sy * self.at.1 as f32,
            ),
            size(sx * self.size.0 as f32, sy * self.size.1 as f32),
        )
    }
}

/// One image made, by key: `None` for a wallpaper that cannot be drawn.
struct Entry {
    key: u64,
    composite: Option<Composite>,
    bytes: usize,
    /// When and where it was last asked for to paint.
    painted: image_budget::PaintStamp,
    /// The windows it was painted in (`painted` counts the main window's
    /// frames only).
    windows: SmallVec<[WindowId; 1]>,
}

impl Entry {
    fn paint_in(&mut self, window: WindowId) {
        self.painted = image_budget::paint_stamp();
        if !self.windows.contains(&window) {
            self.windows.push(window);
        }
    }
}

/// Images made so far (UI thread).
#[derive(Default)]
struct Cache {
    /// Oldest first.
    entries: VecDeque<Entry>,
    bytes: usize,
    /// What each slot showed last (painted stretched while its next size
    /// renders, if it is still cached).
    shown: HashMap<u64, Shown>,
}

/// One slot's last picture.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Shown {
    key: u64,
    /// The wallpaper whatever its rotation.
    identity: u64,
    area: (u32, u32),
    window: WindowId,
}

/// A crossfade from the picture a wallpaper showed before it turned.
struct Fade {
    from: Composite,
    start: Instant,
}

impl Cache {
    /// The image for `key`, to paint now in `window`.
    fn get(&mut self, key: u64, window: WindowId) -> Option<Option<Composite>> {
        let at = self.entries.iter().position(|e| e.key == key)?;
        let mut entry = self.entries.remove(at)?;
        entry.paint_in(window);
        let value = entry.composite.clone();
        self.entries.push_back(entry);
        Some(value)
    }

    /// The image for `key`, if made, to paint now in `window`; its place
    /// in the order stays.
    fn peek(&mut self, key: u64, window: WindowId) -> Option<Composite> {
        let entry = self.entries.iter_mut().find(|e| e.key == key)?;
        entry.paint_in(window);
        entry.composite.clone()
    }

    fn insert(&mut self, key: u64, composite: Option<Composite>, window: WindowId) {
        let bytes = composite
            .as_ref()
            .map_or(0, |c| c.size.0 as usize * c.size.1 as usize * 4);
        self.entries.push_back(Entry {
            key,
            composite,
            bytes,
            painted: image_budget::paint_stamp(),
            windows: SmallVec::from_buf([window]),
        });
        self.bytes += bytes;
        // Images on screen are looked up on every paint, so they sit at
        // the back; the newest one always stays.
        let mut retired = Vec::new();
        while self.bytes > BUDGET_BYTES && self.entries.len() > 1 {
            if let Some(old) = self.entries.pop_front() {
                self.bytes -= old.bytes;
                retired.extend(old.composite.map(|c| c.image));
            }
        }
        image_budget::retire_all(retired);
    }

    /// Take out the images no slot shows and no frame can still show, of
    /// those only `main` (or windows since closed) painted: frames are
    /// counted in the main window only, so another open window may still
    /// show anything it painted. A slot's current picture stays; slots of
    /// closed windows are forgotten.
    fn take_unshown(&mut self, main: WindowId, open: &[WindowId]) -> Vec<Arc<RenderImage>> {
        self.shown.retain(|_, s| open.contains(&s.window));
        let shown: HashSet<u64> = self.shown.values().map(|s| s.key).collect();
        let only_main = |e: &Entry| e.windows.iter().all(|w| *w == main || !open.contains(w));
        let (keep, gone): (VecDeque<Entry>, VecDeque<Entry>) = std::mem::take(&mut self.entries)
            .into_iter()
            .partition(|e| {
                shown.contains(&e.key) || !only_main(e) || image_budget::may_still_show(e.painted)
            });
        self.entries = keep;
        self.bytes = self.entries.iter().map(|e| e.bytes).sum();
        gone.into_iter()
            .filter_map(|e| e.composite.map(|c| c.image))
            .collect()
    }
}

/// Quill went idle: drop the pictures no area shows any more (they render
/// again if asked for). Runs between frames, so they leave every window's
/// atlas now; one a crossfade still holds is retired instead.
pub(super) fn release_unshown(main: WindowId, cx: &mut App) {
    let open: Vec<WindowId> = cx.windows().iter().map(|w| w.window_id()).collect();
    let gone = CACHE.with(|cache| cache.borrow_mut().take_unshown(main, &open));
    if super::frame_clock::trace_ticks() && !gone.is_empty() {
        let bytes: usize = gone
            .iter()
            .map(|image| image.as_bytes(0).map_or(0, <[u8]>::len))
            .sum();
        eprintln!(
            "wallpaper idle release: {} pictures, {:.1} MB",
            gone.len(),
            bytes as f64 / 1048576.0
        );
    }
    let (held, unheld): (Vec<_>, Vec<_>) = gone
        .into_iter()
        .partition(|image| Arc::strong_count(image) > 1);
    image_budget::retire_all(held);
    for image in unheld {
        cx.drop_image(image, None);
    }
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
    /// Running crossfades by wallpaper identity.
    static FADES: RefCell<HashMap<u64, Fade>> = RefCell::new(HashMap::new());
}

/// Record that `slot` now shows `shown`. When only the rotation changed
/// (same wallpaper, same size) and motion is allowed, a crossfade from the
/// previous picture starts: this frame keeps the previous picture, and the
/// caller re-renders so the overlay appears. Returns what to paint and
/// whether a fade started.
fn settle(
    cache: &mut Cache,
    slot: u64,
    shown: Shown,
    composite: Option<Composite>,
) -> (Option<Composite>, bool) {
    let previous = cache.shown.insert(slot, shown);
    if let Some(previous) = previous
        && previous.key != shown.key
        && previous.identity == shown.identity
        && previous.area == shown.area
        && !quill::power_saving::reduce_motion_now()
        && let Some(from) = cache.peek(previous.key, shown.window)
    {
        FADES.with(|f| {
            f.borrow_mut().insert(
                shown.identity,
                Fade {
                    from: from.clone(),
                    start: Instant::now(),
                },
            )
        });
        return (Some(from), true);
    }
    (composite, false)
}

/// Opacity left of the old picture `elapsed` into a fade.
fn fade_opacity(elapsed: Duration) -> Option<f32> {
    (elapsed < FADE).then(|| 1.0 - elapsed.as_secs_f32() / FADE.as_secs_f32())
}

/// The fading-out picture over a wallpaper that just turned, or `None`
/// once the fade is over (render the history again while it runs).
pub(super) fn fade_layer(wallpaper: &Wallpaper) -> Option<AnyElement> {
    let identity = wallpaper.identity();
    let (from, opacity) = FADES.with(|f| {
        let mut fades = f.borrow_mut();
        let fade = fades.get(&identity)?;
        match fade_opacity(fade.start.elapsed()) {
            Some(opacity) => Some((fade.from.clone(), opacity)),
            None => {
                fades.remove(&identity);
                None
            }
        }
    })?;
    Some(
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .opacity(opacity)
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        let _ = window.paint_image(
                            bounds,
                            from.image_bounds(bounds),
                            Corners::default(),
                            from.image.clone(),
                            0,
                            false,
                        );
                    },
                )
                .size_full(),
            )
            .into_any_element(),
    )
}

/// What a canvas paints this frame.
pub(super) struct Lookup {
    pub composite: Option<Composite>,
    /// Ask for another frame: a newer image is on its way, or this one is
    /// new.
    pub pending: bool,
    /// A crossfade started: the history must render its overlay.
    pub fade_started: bool,
}

/// The image for `wallpaper` at `width` x `height` pixels in `slot`.
fn lookup(
    wallpaper: &Wallpaper,
    slot: u64,
    thumb: bool,
    (width, height): (u32, u32),
    window: WindowId,
) -> Lookup {
    let key = image_key(wallpaper, width, height);
    let shown = Shown {
        key,
        identity: if thumb { key } else { wallpaper.identity() },
        area: (width, height),
        window,
    };
    // `made`: the picture is new, which is activity for the idle trim
    // (`release_unshown`); one more frame lets `QuillApp::render`
    // schedule it.
    let ready = |cache: &mut Cache, composite: Option<Composite>, made: bool| {
        if made {
            image_budget::note_activity();
        }
        let (composite, fade_started) = if thumb {
            (composite, false)
        } else {
            settle(cache, slot, shown, composite)
        };
        Lookup {
            composite,
            pending: fade_started || made,
            fade_started,
        }
    };
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(found) = cache.get(key, window) {
            return ready(&mut cache, found, false);
        }
        let shared = shared();
        if !shared.threaded {
            let rendered = INLINE_SOURCES.with(|s| s.borrow_mut().render(wallpaper, width, height));
            let composite = rendered.and_then(|r| to_composite(r, width, height));
            cache.insert(key, composite.clone(), window);
            return ready(&mut cache, composite, true);
        }
        let mut board = shared.board.lock_or_recover();
        if let Some((_, result)) = board.done.remove(&key) {
            drop(board);
            let composite = result.and_then(|r| to_composite(r, width, height));
            cache.insert(key, composite.clone(), window);
            return ready(&mut cache, composite, true);
        }
        // Thumbnails share a slot and never resize: no stale picture.
        let stale = if thumb {
            None
        } else {
            let last = cache.shown.get(&slot).map(|s| s.key);
            last.and_then(|key| cache.peek(key, window))
        };
        let not_before = if stale.is_some() {
            Instant::now() + SETTLE
        } else {
            Instant::now()
        };
        board.request(
            Job {
                key,
                slot,
                wallpaper: wallpaper.clone(),
                width,
                height,
                not_before,
            },
            !thumb,
        );
        drop(board);
        shared.wake.notify_one();
        Lookup {
            composite: stale,
            pending: true,
            fade_started: false,
        }
    })
}

fn to_composite(rendered: Rendered, width: u32, height: u32) -> Option<Composite> {
    let Rendered { raster, x, y } = rendered;
    let size = (raster.width, raster.height);
    let buffer = image::RgbaImage::from_raw(raster.width, raster.height, raster.bgra)?;
    Some(Composite {
        image: Arc::new(RenderImage::new(smallvec::SmallVec::from_buf([
            image::Frame::new(buffer),
        ]))),
        area: (width, height),
        at: (x, y),
        size,
    })
}

/// The element that paints `wallpaper` over its parent's area. `on_area`
/// sees the area's bounds each frame (the history uses it to place
/// gradient bubbles).
pub(super) fn wallpaper_layer(
    wallpaper: Wallpaper,
    slot: Slot,
    on_area: Option<fn(Bounds<Pixels>)>,
) -> impl IntoElement {
    canvas(
        move |bounds, window, cx| {
            if let Some(on_area) = on_area {
                on_area(bounds);
            }
            let scale = window.scale_factor();
            let width = (f32::from(bounds.size.width) * scale).round().max(0.0) as u32;
            let height = (f32::from(bounds.size.height) * scale).round().max(0.0) as u32;
            if width == 0 || height == 0 {
                return None;
            }
            let mut hasher = std::hash::DefaultHasher::new();
            slot.hash(&mut hasher);
            if let Slot::History(_) = slot {
                window
                    .window_handle()
                    .window_id()
                    .as_u64()
                    .hash(&mut hasher);
            }
            let found = lookup(
                &wallpaper,
                hasher.finish(),
                slot == Slot::Thumb,
                (width, height),
                window.window_handle().window_id(),
            );
            if found.fade_started
                && let Slot::History(entity) = slot
            {
                cx.notify(entity);
            }
            Some(found)
        },
        move |bounds, lookup, window, _| {
            let Some(lookup) = lookup else {
                return;
            };
            if let Some(composite) = &lookup.composite {
                let _ = window.paint_image(
                    bounds,
                    composite.image_bounds(bounds),
                    Corners::default(),
                    composite.image.clone(),
                    0,
                    false,
                );
            }
            if lookup.pending {
                window.request_animation_frame();
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

#[cfg(test)]
mod tests {
    use super::{Board, Cache, Composite, Job, Shown, image_key};
    use crate::ui::image_budget::begin_frame;
    use crate::ui::wallpaper::Wallpaper;
    use gpui_kit::{RenderImage, WindowId};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn composite(width: u32) -> Composite {
        Composite {
            image: Arc::new(RenderImage::new(smallvec::SmallVec::from_buf([
                image::Frame::new(image::RgbaImage::new(width, 1)),
            ]))),
            area: (width, 1),
            at: (0, 0),
            size: (width, 1),
        }
    }

    const MAIN: u64 = 1;
    const CHAT: u64 = 2;

    fn window(id: u64) -> WindowId {
        WindowId::from(id)
    }

    /// What the idle release takes out, by picture width (= key), with
    /// `open` windows open.
    fn released(cache: &mut Cache, open: &[u64]) -> Vec<u32> {
        let open: Vec<WindowId> = open.iter().copied().map(window).collect();
        let mut gone: Vec<u32> = cache
            .take_unshown(window(MAIN), &open)
            .iter()
            .map(|image| image.size(0).width.0 as u32)
            .collect();
        gone.sort_unstable();
        gone
    }

    fn show(cache: &mut Cache, slot: u64, key: u64, in_window: u64) {
        let shown = Shown {
            key,
            identity: 0,
            area: (key as u32, 1),
            window: window(in_window),
        };
        cache.shown.insert(slot, shown);
    }

    #[test]
    fn once_idle_only_what_a_slot_shows_stays() {
        let mut cache = Cache::default();
        // A window resized through three widths, then a closed dialog's
        // thumbnail; the history slot shows the last width.
        for key in 1..=4 {
            cache.insert(key, Some(composite(key as u32)), window(MAIN));
        }
        show(&mut cache, 7, 3, MAIN);
        begin_frame();
        begin_frame();

        assert_eq!(released(&mut cache, &[MAIN]), [1, 2, 4]);
        assert_eq!(cache.entries.len(), 1);
        assert_eq!(cache.bytes, 3 * 4);
        assert!(cache.get(3, window(MAIN)).is_some());
    }

    #[test]
    fn a_picture_painted_last_frame_stays() {
        let mut cache = Cache::default();
        begin_frame();
        cache.insert(1, Some(composite(1)), window(MAIN));
        assert!(released(&mut cache, &[MAIN]).is_empty());
        assert_eq!(cache.entries.len(), 1);
    }

    #[test]
    fn what_another_open_window_painted_stays_until_it_closes() {
        let mut cache = Cache::default();
        // A thumbnail in a chat window, and that window's history.
        cache.insert(1, Some(composite(1)), window(CHAT));
        cache.insert(2, Some(composite(2)), window(CHAT));
        show(&mut cache, 8, 2, CHAT);
        begin_frame();
        begin_frame();
        assert!(released(&mut cache, &[MAIN, CHAT]).is_empty());

        // The chat window closed.
        assert_eq!(released(&mut cache, &[MAIN]), [1, 2]);
        assert!(cache.shown.is_empty());
    }

    fn job(key: u64, slot: u64, delay_ms: u64) -> Job {
        Job {
            key,
            slot,
            wallpaper: Wallpaper::Solid(0),
            width: 1,
            height: 1,
            not_before: Instant::now() + Duration::from_millis(delay_ms),
        }
    }

    #[test]
    fn fades_run_for_two_hundred_milliseconds() {
        use super::fade_opacity;
        assert_eq!(fade_opacity(Duration::ZERO), Some(1.0));
        let half = fade_opacity(Duration::from_millis(100)).unwrap();
        assert!((half - 0.5).abs() < 1e-3);
        assert_eq!(fade_opacity(Duration::from_millis(200)), None);
    }

    #[test]
    fn keys_change_with_the_wallpaper_and_the_size() {
        let a = Wallpaper::Solid(1);
        assert_eq!(image_key(&a, 10, 10), image_key(&a, 10, 10));
        assert_ne!(image_key(&a, 10, 10), image_key(&a, 10, 11));
        assert_ne!(
            image_key(&a, 10, 10),
            image_key(&Wallpaper::Solid(2), 10, 10)
        );
    }

    #[test]
    fn a_slot_keeps_only_its_newest_request() {
        let mut board = Board::default();
        board.request(job(1, 7, 0), true);
        board.request(job(2, 7, 0), true);
        board.request(job(3, 8, 0), true);
        assert_eq!(board.queue.len(), 2);
        assert!(!board.wanted.contains(&1));
        // Thumbnails do not replace each other.
        board.request(job(4, 9, 0), false);
        board.request(job(5, 9, 0), false);
        assert_eq!(board.queue.len(), 4);
        // The same key twice is one job.
        board.request(job(5, 9, 0), false);
        assert_eq!(board.queue.len(), 4);
    }

    #[test]
    fn superseded_renders_are_dropped_and_settling_jobs_wait() {
        let mut board = Board::default();
        board.request(job(1, 7, 0), true);
        let running = board.next(Instant::now()).ok().unwrap();
        assert_eq!(running.key, 1);
        // The window keeps resizing while key 1 renders.
        board.request(job(2, 7, 10_000), true);
        board.finish(7, 1, None);
        assert!(!board.done.contains_key(&1), "nobody wants it any more");
        match board.next(Instant::now()) {
            Err(Some(wait)) => assert!(wait > Duration::from_secs(5)),
            _ => panic!("the resize has not settled"),
        }
        board.request(job(3, 8, 0), true);
        assert_eq!(board.next(Instant::now()).ok().unwrap().key, 3);
        board.finish(8, 3, None);
        assert!(board.done.contains_key(&3));
    }

    #[test]
    fn a_result_its_slot_moved_past_is_dropped() {
        let mut board = Board::default();
        board.request(job(1, 7, 0), true);
        board.request(job(2, 9, 0), false);
        for _ in 0..2 {
            let running = board.next(Instant::now()).ok().unwrap();
            board.finish(running.slot, running.key, None);
        }
        assert!(board.done.contains_key(&1));
        // The window is at a new size before a frame took key 1.
        board.request(job(3, 7, 0), true);
        assert!(!board.done.contains_key(&1), "nobody will ask for it");
        // Thumbnails share a slot and never replace each other.
        board.request(job(4, 9, 0), false);
        assert!(board.done.contains_key(&2));
    }
}
