//! The state the UI and the workers share: open clips, their stores, the
//! frames asked for and the frames drawn.
#![deny(clippy::indexing_slicing)]

use super::store::StoredFrame;
use super::{Cancel, ClipInfo, ClipKey, ClipState, Origin, Rendered};
use crate::locks::lock_or_recover;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// A clip drawn this recently keeps its stored frames under pressure:
/// evicting what is on screen would only draw it again next frame.
const ON_SCREEN: Duration = Duration::from_millis(500);
/// Frames asked for at once, per clip.
const MAX_WANTED: usize = 8;

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
}

pub(super) struct Clip {
    pub(super) worker: usize,
    /// Set until a worker takes it to open the clip.
    pub(super) origin: Option<Origin>,
    pub(super) state: ClipState,
    pub(super) store: HashMap<u32, StoredFrame>,
    pub(super) store_bytes: usize,
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
}

pub(super) enum Job {
    Open {
        key: ClipKey,
        origin: Origin,
    },
    Frame {
        key: ClipKey,
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
    pub(super) stop: bool,
    pub(super) budget: usize,
    pub(super) store_bytes: usize,
    pub(super) stats: EngineStats,
    served: u64,
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
                stop: false,
                budget,
                store_bytes: 0,
                stats: EngineStats::default(),
                served: 0,
            }),
            work: Condvar::new(),
            waker: Mutex::new(None),
        });
        let mut started = 0;
        for index in 0..workers {
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
        state.clips.insert(
            key,
            Clip {
                worker,
                origin: Some(origin),
                state: ClipState::Opening,
                store: HashMap::new(),
                store_bytes: 0,
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
    pub(super) fn close(&mut self, key: &ClipKey) {
        let Some(clip) = self.clips.remove(key) else {
            return;
        };
        self.store_bytes = self.store_bytes.saturating_sub(clip.store_bytes);
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
                        && clip.store_bytes > 0
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
                self.store_bytes = self.store_bytes.saturating_sub(clip.store_bytes);
                clip.store.clear();
                clip.store_bytes = 0;
            }
        }
    }

    /// Stores `frame` as `key`'s frame `index` if it fits the budget (a
    /// store-only clip's frames always go in: they are its source).
    pub(super) fn store(&mut self, key: &ClipKey, index: u32, frame: StoredFrame) {
        let bytes = frame.bytes();
        let Some(store_only) = self.clips.get(key).map(|clip| clip.store_only) else {
            return;
        };
        if !self.make_room(bytes, key) && !store_only {
            return;
        }
        let Some(clip) = self.clips.get_mut(key) else {
            return;
        };
        if let Some(old) = clip.store.insert(index, frame) {
            clip.store_bytes = clip.store_bytes.saturating_sub(old.bytes());
            self.store_bytes = self.store_bytes.saturating_sub(old.bytes());
        }
        clip.store_bytes = clip.store_bytes.saturating_add(bytes);
        self.store_bytes = self.store_bytes.saturating_add(bytes);
    }

    /// The next job for worker `me`: a clip to open, else the soonest
    /// frame of the clip served longest ago.
    pub(super) fn next_job(&mut self, me: usize) -> Option<Job> {
        if let Some((key, clip)) = self
            .clips
            .iter_mut()
            .find(|(_, clip)| clip.worker == me && !clip.busy && clip.origin.is_some())
        {
            let origin = clip.origin.take()?;
            clip.busy = true;
            return Some(Job::Open { key: *key, origin });
        }
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
        let stored = clip.store.get(&index).cloned();
        Some(Job::Frame { key, index, stored })
    }
}
