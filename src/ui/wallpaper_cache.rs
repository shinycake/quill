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

use super::image_budget;
use super::wallpaper::Wallpaper;
use super::wallpaper_source::{Rendered, Sources};
use gpui_kit::*;
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
    done: HashMap<u64, Option<Rendered>>,
}

impl Board {
    /// Queue `job`, replacing the slot's older request.
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

    fn finish(&mut self, key: u64, result: Option<Rendered>) {
        self.running = None;
        if self.wanted.remove(&key) {
            if self.done.len() >= DONE_CAP {
                self.done.clear();
            }
            self.done.insert(key, result);
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
        let threaded = std::thread::Builder::new()
            .name("quill-wallpaper".into())
            .spawn(worker)
            .is_ok();
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
            let mut board = shared.board.lock().unwrap_or_else(|e| e.into_inner());
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
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .finish(job.key, result);
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

/// Images made so far (UI thread).
#[derive(Default)]
struct Cache {
    /// Oldest first; `None` for a wallpaper that cannot be drawn.
    entries: VecDeque<(u64, Option<Composite>, usize)>,
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
}

/// A crossfade from the picture a wallpaper showed before it turned.
struct Fade {
    from: Composite,
    start: Instant,
}

impl Cache {
    fn get(&mut self, key: u64) -> Option<Option<Composite>> {
        let at = self.entries.iter().position(|(k, _, _)| *k == key)?;
        let entry = self.entries.remove(at)?;
        let value = entry.1.clone();
        self.entries.push_back(entry);
        Some(value)
    }

    fn peek(&self, key: u64) -> Option<Composite> {
        self.entries
            .iter()
            .find(|(k, _, _)| *k == key)
            .and_then(|(_, c, _)| c.clone())
    }

    fn insert(&mut self, key: u64, composite: Option<Composite>) {
        let bytes = composite
            .as_ref()
            .map_or(0, |c| c.size.0 as usize * c.size.1 as usize * 4);
        self.entries.push_back((key, composite, bytes));
        self.bytes += bytes;
        // Images on screen are looked up on every paint, so they sit at
        // the back; the newest one always stays.
        let mut retired = Vec::new();
        while self.bytes > BUDGET_BYTES && self.entries.len() > 1 {
            if let Some((_, old, bytes)) = self.entries.pop_front() {
                self.bytes -= bytes;
                retired.extend(old.map(|c| c.image));
            }
        }
        image_budget::retire_all(retired);
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
        && let Some(from) = cache.peek(previous.key)
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
    /// A newer image is on its way; ask for another frame.
    pub pending: bool,
    /// A crossfade started: the history must render its overlay.
    pub fade_started: bool,
}

/// The image for `wallpaper` at `width` x `height` pixels in `slot`.
fn lookup(wallpaper: &Wallpaper, slot: u64, thumb: bool, width: u32, height: u32) -> Lookup {
    let key = image_key(wallpaper, width, height);
    let shown = Shown {
        key,
        identity: if thumb { key } else { wallpaper.identity() },
        area: (width, height),
    };
    let ready = |cache: &mut Cache, composite: Option<Composite>| {
        let (composite, fade_started) = if thumb {
            (composite, false)
        } else {
            settle(cache, slot, shown, composite)
        };
        Lookup {
            composite,
            pending: fade_started,
            fade_started,
        }
    };
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(found) = cache.get(key) {
            return ready(&mut cache, found);
        }
        let shared = shared();
        if !shared.threaded {
            let rendered = INLINE_SOURCES.with(|s| s.borrow_mut().render(wallpaper, width, height));
            let composite = rendered.and_then(|r| to_composite(r, width, height));
            cache.insert(key, composite.clone());
            return ready(&mut cache, composite);
        }
        let mut board = shared.board.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(result) = board.done.remove(&key) {
            drop(board);
            let composite = result.and_then(|r| to_composite(r, width, height));
            cache.insert(key, composite.clone());
            return ready(&mut cache, composite);
        }
        // Thumbnails share a slot and never resize: no stale picture.
        let stale = if thumb {
            None
        } else {
            cache.shown.get(&slot).and_then(|s| cache.peek(s.key))
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
                width,
                height,
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
    use super::{Board, Job, image_key};
    use crate::ui::wallpaper::Wallpaper;
    use std::time::{Duration, Instant};

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
        board.finish(1, None);
        assert!(!board.done.contains_key(&1), "nobody wants it any more");
        match board.next(Instant::now()) {
            Err(Some(wait)) => assert!(wait > Duration::from_secs(5)),
            _ => panic!("the resize has not settled"),
        }
        board.request(job(3, 8, 0), true);
        assert_eq!(board.next(Instant::now()).ok().unwrap().key, 3);
        board.finish(3, None);
        assert!(board.done.contains_key(&3));
    }
}
