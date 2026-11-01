//! The state the UI and the workers share: open clips, their stores, the
//! frames asked for and the frames drawn.
#![deny(clippy::indexing_slicing)]

use super::store::{Store, StoredFrame};
use super::{Cancel, ClipInfo, ClipKey, ClipState, DecodeAll, Origin, Rendered, Timeline};
use crate::locks::lock_or_recover;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// A clip drawn this recently keeps its stored frames under pressure:
/// evicting what is on screen would only draw it again next frame.
const ON_SCREEN: Duration = Duration::from_millis(500);
/// Frames asked for at once, per clip.
const MAX_WANTED: usize = 8;
/// Frames a worker draws in a row while a clip waits for it to open.
const FRAMES_BEFORE_OPEN: u32 = 8;

/// What the engine did, for the decision doc's measurements and
/// `QUILL_TRACE_STICKERS`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EngineStats {
    /// Frames ThorVG drew, and the time it took.
    pub drawn: u64,
    pub draw_time: Duration,
    /// Frames unpacked from a store, and the time it took.
    pub unpacked: u64,
    pub unpack_time: Duration,
    /// Time spent packing drawn frames into the store.
    pub pack_time: Duration,
    /// Bytes held by the stores, and by the open clips' sources.
    pub store_bytes: usize,
    pub source_bytes: usize,
    pub clips: usize,
    /// Stores dropped to stay under the budget.
    pub evicted: u64,
    /// Open clips held to their file's rate.
    pub coarse: usize,
}

pub(super) struct Clip {
    /// Tells this opening of its key from one closed before: work handed
    /// out for that one must not land in this one.
    pub(super) id: u64,
    pub(super) worker: usize,
    /// Set until a worker takes it to open the clip.
    pub(super) origin: Option<Origin>,
    pub(super) state: ClipState,
    pub(super) store: Store,
    /// Frames decoded up front live only in the store: dropping it closes
    /// the clip.
    pub(super) store_only: bool,
    /// What its source holds besides the store (canvas, last frame).
    pub(super) source_bytes: usize,
    pub(super) wanted: VecDeque<u32>,
    /// A worker is on it, and the frame it is drawing.
    pub(super) busy: bool,
    pub(super) drawing: Option<u32>,
    /// A frame of it was handed out.
    pub(super) delivered: bool,
    pub(super) used: Instant,
    /// Round-robin order among a worker's clips.
    pub(super) served: u64,
    pub(super) cancel: Cancel,
    pub(super) draw_cost: DrawCost,
    /// Its frames draw too slowly for the fine grid: it plays at its
    /// file's own rate from then on, so it never falls behind.
    pub(super) coarse: bool,
}

impl Clip {
    /// Notes that one of its frames took `draw` to draw.
    pub(super) fn note_draw(&mut self, draw: Duration) {
        if let ClipState::Ready(info) = &self.state {
            self.coarse |= self.draw_cost.note(draw, info.timeline);
        }
    }
}

/// Draws averaged before a clip may drop to its file's rate.
const SETTLED: u32 = 8;

/// How long a clip's frames take to draw, averaged slowly enough that one
/// cold or preempted draw can't pass for a slow clip.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct DrawCost {
    /// Draws seen, the first included.
    seen: u32,
    average: Duration,
}

impl DrawCost {
    /// Notes one draw; true once the average settled too slow for
    /// `timeline`'s fine grid. The first draw is left out: it opens the
    /// canvas and runs cold.
    pub(super) fn note(&mut self, draw: Duration, timeline: Timeline) -> bool {
        self.seen = self.seen.saturating_add(1);
        let Some(averaged) = self.seen.checked_sub(1).filter(|n| *n > 0) else {
            return false;
        };
        let window = averaged.min(SETTLED);
        self.average = (self.average * (window - 1) + draw) / window;
        averaged >= SETTLED && too_slow_for_grid(self.average, timeline)
    }
}

/// Whether frames that take `draw_cost` to draw can't keep up with
/// `timeline`'s fine grid. A worker draws one clip's frames one at a time,
/// and needs room for the other clips it owns and for slower frames.
pub(super) fn too_slow_for_grid(draw_cost: Duration, timeline: Timeline) -> bool {
    let grid = timeline.grid_fps();
    timeline.step > 1
        && grid.is_finite()
        && grid > 0.0
        && draw_cost > Duration::from_secs_f64(COARSE_SHARE / grid)
}

/// The share of a grid frame's time one frame may take to draw before its
/// clip drops to the file's own rate.
const COARSE_SHARE: f64 = 0.4;

pub(super) enum Job {
    /// Open a TGS clip (decoded clips open on the opener threads).
    Open {
        key: ClipKey,
        id: u64,
        path: PathBuf,
    },
    Frame {
        key: ClipKey,
        id: u64,
        index: u32,
        /// The frame's stored data, if it was drawn before.
        stored: Option<StoredFrame>,
    },
}

pub(super) struct State {
    pub(super) clips: HashMap<ClipKey, Clip>,
    pub(super) rendered: Vec<Rendered>,
    /// Clips each worker must drop.
    pub(super) closed: Vec<Vec<ClipKey>>,
    /// Stores of closed clips and evicted ones, for a worker to free
    /// outside the lock: each block a large allocation going back to the
    /// system, which the UI thread (closing clips) should not wait for.
    pub(super) freed: Vec<Store>,
    pub(super) stop: bool,
    pub(super) budget: usize,
    pub(super) store_bytes: usize,
    pub(super) stats: EngineStats,
    served: u64,
    /// The last clip id handed out.
    opened: u64,
    /// Frames each worker drew in a row while one of its clips waited to
    /// open.
    frames_in_a_row: Vec<u32>,
}

pub(super) struct Shared {
    pub(super) state: Mutex<State>,
    pub(super) work: Condvar,
    pub(super) waker: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

impl Shared {
    /// Tell the UI frames are ready.
    pub(super) fn wake(&self) {
        let waker = lock_or_recover(&self.waker).clone();
        if let Some(waker) = waker {
            waker();
        }
    }
}

/// The player's engine. Cheap to share; dropping the last handle stops
/// the workers.
pub struct Engine {
    shared: Arc<Shared>,
    workers: usize,
}

impl Engine {
    /// An engine with `workers` threads (at least one) and `budget` bytes
    /// for stores and sources together.
    pub fn new(workers: usize, budget: usize) -> Self {
        let workers = workers.max(1);
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                clips: HashMap::new(),
                rendered: Vec::new(),
                closed: vec![Vec::new(); workers],
                freed: Vec::new(),
                stop: false,
                budget,
                store_bytes: 0,
                stats: EngineStats::default(),
                served: 0,
                opened: 0,
                frames_in_a_row: vec![0; workers],
            }),
            work: Condvar::new(),
            waker: Mutex::new(None),
        });
        let mut started = 0;
        for index in 0..workers {
            let opener = shared.clone();
            let _ = crate::thread_guard::spawn_guarded(
                format!("quill-sticker-open-{index}"),
                move || super::opener::run(&opener),
            );
            let shared = shared.clone();
            let spawned =
                crate::thread_guard::spawn_guarded(format!("quill-sticker-{index}"), move || {
                    super::worker::run(index, &shared)
                });
            if spawned.is_ok() {
                started += 1;
            }
        }
        Self {
            shared,
            workers: started.max(1),
        }
    }

    /// Worker threads for this machine: three with eight or more cores,
    /// else two.
    pub fn default_workers() -> usize {
        match std::thread::available_parallelism() {
            Ok(cores) if cores.get() >= 8 => 3,
            _ => 2,
        }
    }

    /// Called (on a worker thread) whenever frames are ready or a clip
    /// opened or failed.
    pub fn set_waker(&self, waker: Arc<dyn Fn() + Send + Sync>) {
        *lock_or_recover(&self.shared.waker) = Some(waker);
    }

    /// Opens `key` from `origin` unless it is open.
    pub fn open(&self, key: ClipKey, origin: Origin) {
        let mut state = lock_or_recover(&self.shared.state);
        if let Some(clip) = state.clips.get_mut(&key) {
            clip.used = Instant::now();
            return;
        }
        let mut load = vec![0usize; self.workers];
        for clip in state.clips.values() {
            if let Some(count) = load.get_mut(clip.worker) {
                *count += 1;
            }
        }
        let worker = (0..self.workers)
            .min_by_key(|index| load.get(*index).copied().unwrap_or(0))
            .unwrap_or(0);
        let store_only = matches!(origin, Origin::Decoded(_));
        state.opened += 1;
        let id = state.opened;
        state.clips.insert(
            key,
            Clip {
                id,
                worker,
                origin: Some(origin),
                state: ClipState::Opening,
                store: Store::default(),
                store_only,
                source_bytes: 0,
                // Its first frame, for the still a view waits on.
                wanted: VecDeque::from([0]),
                busy: false,
                drawing: None,
                delivered: false,
                used: Instant::now(),
                served: 0,
                cancel: Cancel::default(),
                draw_cost: DrawCost::default(),
                coarse: false,
            },
        );
        drop(state);
        self.shared.work.notify_all();
    }

    /// The state of `key`, if it is open.
    pub fn state(&self, key: &ClipKey) -> Option<ClipState> {
        lock_or_recover(&self.shared.state)
            .clips
            .get(key)
            .map(|clip| clip.state.clone())
    }

    /// Whether `key` plays only at its file's own rate (its frames draw
    /// too slowly for the fine grid).
    pub fn coarse(&self, key: &ClipKey) -> bool {
        lock_or_recover(&self.shared.state)
            .clips
            .get(key)
            .is_some_and(|clip| clip.coarse)
    }

    /// The opened clip's frames, if it opened.
    pub fn info(&self, key: &ClipKey) -> Option<ClipInfo> {
        match self.state(key)? {
            ClipState::Ready(info) => Some(info),
            _ => None,
        }
    }

    /// Ask for frames of `key`, soonest first, in place of what was asked
    /// before. Frames drawn but not taken yet are not asked again.
    pub fn want(&self, key: &ClipKey, frames: &[u32]) {
        let mut state = lock_or_recover(&self.shared.state);
        let State {
            clips, rendered, ..
        } = &mut *state;
        let Some(clip) = clips.get_mut(key) else {
            return;
        };
        clip.used = Instant::now();
        clip.wanted.clear();
        for index in frames.iter().copied() {
            let pending = clip.drawing == Some(index)
                || rendered
                    .iter()
                    .any(|frame| frame.key == *key && frame.index == index);
            if !pending && !clip.wanted.contains(&index) && clip.wanted.len() < MAX_WANTED {
                clip.wanted.push_back(index);
            }
        }
        let work = !clip.wanted.is_empty() && !clip.busy;
        drop(state);
        if work {
            self.shared.work.notify_all();
        }
    }

    /// Whether the workers have nothing to do: every clip opened and no
    /// frame is asked for or being drawn.
    pub fn idle(&self) -> bool {
        lock_or_recover(&self.shared.state)
            .clips
            .values()
            .all(|clip| clip.origin.is_none() && !clip.busy && clip.wanted.is_empty())
    }

    /// Mark `key` as shown now, so its store is kept.
    pub fn touch(&self, key: &ClipKey) {
        if let Some(clip) = lock_or_recover(&self.shared.state).clips.get_mut(key) {
            clip.used = Instant::now();
        }
    }

    /// The frames drawn since the last call.
    pub fn take_rendered(&self) -> Vec<Rendered> {
        std::mem::take(&mut lock_or_recover(&self.shared.state).rendered)
    }

    /// Closes `key`: its worker drops the source, its store goes.
    pub fn close(&self, key: &ClipKey) {
        let mut state = lock_or_recover(&self.shared.state);
        state.close(key);
        drop(state);
        self.shared.work.notify_all();
    }

    /// The open clips, least recently used first.
    pub fn open_clips(&self) -> Vec<(ClipKey, Instant)> {
        let mut clips: Vec<(ClipKey, Instant)> = lock_or_recover(&self.shared.state)
            .clips
            .iter()
            .map(|(key, clip)| (*key, clip.used))
            .collect();
        clips.sort_by_key(|(_, used)| *used);
        clips
    }

    pub fn stats(&self) -> EngineStats {
        let state = lock_or_recover(&self.shared.state);
        let mut stats = state.stats;
        stats.store_bytes = state.store_bytes;
        stats.source_bytes = state.clips.values().map(|clip| clip.source_bytes).sum();
        stats.clips = state.clips.len();
        stats.coarse = state.clips.values().filter(|clip| clip.coarse).count();
        stats
    }

    /// Bytes held by stores and sources against the budget.
    pub fn held_bytes(&self) -> usize {
        let stats = self.stats();
        stats.store_bytes.saturating_add(stats.source_bytes)
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let mut state = lock_or_recover(&self.shared.state);
        state.stop = true;
        let keys: Vec<ClipKey> = state.clips.keys().copied().collect();
        for key in keys {
            state.close(&key);
        }
        drop(state);
        self.shared.work.notify_all();
    }
}

impl State {
    /// `key`'s clip, if it is still the opening `id` names.
    pub(super) fn clip_mut(&mut self, key: &ClipKey, id: u64) -> Option<&mut Clip> {
        self.clips.get_mut(key).filter(|clip| clip.id == id)
    }

    pub(super) fn close(&mut self, key: &ClipKey) {
        let Some(clip) = self.clips.remove(key) else {
            return;
        };
        self.store_bytes = self.store_bytes.saturating_sub(clip.store.bytes());
        self.freed.push(clip.store);
        clip.cancel
            .flag
            .store(true, std::sync::atomic::Ordering::SeqCst);
        if let Ok(mut child) = clip.cancel.child.lock()
            && let Some(child) = child.as_mut()
        {
            let _ = child.kill();
        }
        self.rendered.retain(|frame| frame.key != *key);
        if let Some(closed) = self.closed.get_mut(clip.worker) {
            closed.push(*key);
        }
    }

    /// Bytes held against the budget.
    fn held(&self) -> usize {
        self.clips
            .values()
            .map(|clip| clip.source_bytes)
            .fold(self.store_bytes, usize::saturating_add)
    }

    /// Drop the stores of clips not shown lately, least recently shown
    /// first, until `adding` more bytes fit `keep`'s. False when what is
    /// left is all on screen.
    pub(super) fn make_room(&mut self, adding: usize, keep: &ClipKey) -> bool {
        let now = Instant::now();
        loop {
            if self.held().saturating_add(adding) <= self.budget {
                return true;
            }
            let victim = self
                .clips
                .iter()
                .filter(|(key, clip)| {
                    *key != keep
                        && !clip.busy
                        && clip.store.bytes() > 0
                        && now.saturating_duration_since(clip.used) >= ON_SCREEN
                })
                .min_by_key(|(_, clip)| clip.used)
                .map(|(key, clip)| (*key, clip.store_only));
            let Some((key, store_only)) = victim else {
                return false;
            };
            self.stats.evicted += 1;
            if store_only {
                self.close(&key);
            } else if let Some(clip) = self.clips.get_mut(&key) {
                let store = std::mem::take(&mut clip.store);
                self.store_bytes = self.store_bytes.saturating_sub(store.bytes());
                self.freed.push(store);
            }
        }
    }

    /// Stores `packed` as `key`'s frame `index` if it fits the budget.
    pub(super) fn store(&mut self, key: &ClipKey, index: u32, packed: &[u8]) {
        if !self.clips.contains_key(key) || !self.make_room(packed.len(), key) {
            return;
        }
        let Some(clip) = self.clips.get_mut(key) else {
            return;
        };
        let before = clip.store.bytes();
        clip.store.insert(index, packed);
        let added = clip.store.bytes().saturating_sub(before);
        self.store_bytes = self.store_bytes.saturating_add(added);
    }

    /// Gives a clip decoded up front its frames, `store`: they are its
    /// source, so they go in whatever the budget.
    pub(super) fn install(&mut self, key: &ClipKey, store: Store) {
        self.make_room(store.bytes(), key);
        let Some(clip) = self.clips.get_mut(key) else {
            self.freed.push(store);
            return;
        };
        self.store_bytes = self.store_bytes.saturating_add(store.bytes());
        let old = std::mem::replace(&mut clip.store, store);
        if old.bytes() > 0 {
            self.store_bytes = self.store_bytes.saturating_sub(old.bytes());
            self.freed.push(old);
        }
    }

    /// The next job for worker `me`: the soonest frame of its clip served
    /// longest ago, else a TGS clip to open. A clip waiting to open goes
    /// first once [`FRAMES_BEFORE_OPEN`] frames went before it.
    pub(super) fn next_job(&mut self, me: usize) -> Option<Job> {
        let opening = self.clips.iter().find_map(|(key, clip)| {
            let tgs = matches!(clip.origin, Some(Origin::Lottie(_)));
            (clip.worker == me && !clip.busy && tgs).then_some(*key)
        });
        let in_a_row = self.frames_in_a_row.get(me).copied().unwrap_or(0);
        let frames_first = opening.is_none() || in_a_row < FRAMES_BEFORE_OPEN;
        let job = frames_first.then(|| self.next_frame(me)).flatten();
        if let Some(count) = self.frames_in_a_row.get_mut(me) {
            *count = match (&job, opening) {
                (Some(_), Some(_)) => count.saturating_add(1),
                _ => 0,
            };
        }
        if job.is_some() {
            return job;
        }
        let key = opening?;
        let clip = self.clips.get_mut(&key)?;
        let Some(Origin::Lottie(path)) = clip.origin.take() else {
            return None;
        };
        clip.busy = true;
        Some(Job::Open {
            key,
            id: clip.id,
            path,
        })
    }

    /// The soonest frame worker `me` has to draw, of its clip served
    /// longest ago.
    fn next_frame(&mut self, me: usize) -> Option<Job> {
        let key = self
            .clips
            .iter()
            .filter(|(_, clip)| {
                clip.worker == me
                    && !clip.busy
                    && !clip.wanted.is_empty()
                    && matches!(clip.state, ClipState::Ready(_))
            })
            .min_by_key(|(_, clip)| clip.served)
            .map(|(key, _)| *key)?;
        self.served += 1;
        let served = self.served;
        let clip = self.clips.get_mut(&key)?;
        let index = clip.wanted.pop_front()?;
        clip.busy = true;
        clip.drawing = Some(index);
        clip.served = served;
        let stored = clip.store.get(index);
        Some(Job::Frame {
            key,
            id: clip.id,
            index,
            stored,
        })
    }

    /// The next clip for an opener to decode, with its id and what stops
    /// it.
    pub(super) fn next_decode(&mut self) -> Option<(ClipKey, u64, DecodeAll, Cancel)> {
        let (key, clip) = self
            .clips
            .iter_mut()
            .find(|(_, clip)| !clip.busy && matches!(clip.origin, Some(Origin::Decoded(_))))?;
        let Some(Origin::Decoded(decode)) = clip.origin.take() else {
            return None;
        };
        clip.busy = true;
        Some((*key, clip.id, decode, clip.cancel.clone()))
    }
}
