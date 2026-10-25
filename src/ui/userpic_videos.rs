//! Video userpics beside messages and in the chat list, after tdesktop's
//! `Dialogs::Ui::VideoUserpic` (dialogs_video_userpic.cpp): a Premium
//! user's video photo plays its small clip, muted and looping from the
//! start, in place of the still userpic, which shows until the first
//! picture is decoded.
//!
//! One player per surface and user ([`Key`]); tdesktop also keeps one
//! map per widget (`HistoryInner`, `ListWidget`, `Dialogs::InnerWidget`).
//! The players reuse the inline clip path (`Purpose::Inline` decoding at
//! the userpic's size in device pixels on a decoder thread) and are drawn
//! by their slice's animation layer (`anim_layer::tile`), whose
//! `video_frame` asks the frame clock for the next picture, so a playing
//! userpic redraws only the layer.
//!
//! - Only what is painted decodes. A sweep after each surface pass (and
//!   every [`RELEASE_AFTER`] while a player is awake) pauses the players
//!   not painted in view since the last sweep and puts them to sleep (no
//!   decoder) after [`RELEASE_AFTER`] off screen; one painted again gets
//!   a new player. tdesktop's clip reader stops the same way once its
//!   frames are not shown (`kWaitBeforeGifPause`).
//! - At most [`MAX_AWAKE`] players hold a decoder; past it the one off
//!   screen longest sleeps first, and with none off screen a further
//!   userpic keeps its still.
//! - A slot whose row did not render in its surface's previous pass goes.
//! - Everything holds still while the window is in the background or a
//!   layer, a menu panel or a round video with sound is over the chat
//!   (tdesktop passes `isGifPausedAtLeastFor(GifPauseReason::Any)` as
//!   `paused`).
//!
//! The state sits in one `RefCell`; every borrow is one short scope, and
//! no player call (each reaches the backend) is made while it is held.
#![deny(clippy::indexing_slicing)]

use super::inline_players::{OffScreenStep, RELEASE_AFTER, off_screen_step};
use super::video_player::{PlayState, Purpose, VideoPlayer, video_frame};
use gpui_kit::*;
use quill::ids::FileId;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

/// Where a userpic plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Surface {
    /// Beside messages in the history.
    History,
    /// Chat list rows.
    ChatList,
}

impl Surface {
    fn index(self) -> usize {
        match self {
            Surface::History => 0,
            Surface::ChatList => 1,
        }
    }
}

/// `(surface, user id)`.
pub(super) type Key = (Surface, i64);

/// Players holding a decoder at most, every surface together: a tall
/// window shows about a dozen userpics per surface.
pub(super) const MAX_AWAKE: usize = 24;

/// The largest edge a userpic clip is decoded at: TDLib's small clip is
/// 160 px, the large one up to 1280.
const MAX_EDGE: u32 = 320;
const MIN_EDGE: u32 = 16;

/// The longer side, in device pixels, to decode a `size` pt userpic at on
/// a `scale` display (2x before the first frame reports a scale).
pub(super) fn decode_edge(size: f32, scale: f32) -> u32 {
    let scale = if scale.is_finite() && scale > 0. {
        scale
    } else {
        2.
    };
    (size * scale)
        .ceil()
        .clamp(MIN_EDGE as f32, MAX_EDGE as f32) as u32
}

/// Whether the slot of a row last rendered in pass `seen` stays as pass
/// `pass` of its surface begins: its row rendered in the previous one.
pub(super) fn survives(seen: u64, pass: u64) -> bool {
    seen.saturating_add(1) >= pass
}

/// What opening one more player takes, given the awake slots (with since
/// when each is off screen).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Admit<K> {
    Open,
    /// Put this slot, off screen longest, to sleep first.
    Evict(K),
    /// Every awake slot is on screen: keep the still.
    Refuse,
}

pub(super) fn admit<K: Copy>(awake: &[(K, Option<Instant>)], cap: usize) -> Admit<K> {
    if awake.len() < cap {
        return Admit::Open;
    }
    awake
        .iter()
        .filter_map(|(key, hidden)| hidden.map(|since| (since, *key)))
        .min_by_key(|(since, _)| *since)
        .map_or(Admit::Refuse, |(_, key)| Admit::Evict(key))
}

struct Slot {
    player: VideoPlayer,
    clip: FileId,
    path: PathBuf,
    max_edge: u32,
    /// The surface pass its row last rendered in.
    seen: u64,
    /// The player's paint count at the last sweep: more since means it
    /// was painted in view.
    shown_mark: u64,
    /// Off screen since then: paused, asleep after [`RELEASE_AFTER`].
    hidden_since: Option<Instant>,
    /// No backend (`VideoPlayer::hibernate`) until painted in view again.
    asleep: bool,
}

impl Slot {
    /// A new player's slot, held as if off screen since `now`: the sweep
    /// after its pass plays it once its picture was painted in view, so a
    /// row built only in the list's overdraw never plays.
    fn held_until_painted(
        player: VideoPlayer,
        clip: FileId,
        path: PathBuf,
        max_edge: u32,
        now: Instant,
    ) -> Self {
        Self {
            shown_mark: player.shown_count(),
            player,
            clip,
            path,
            max_edge,
            seen: 0,
            hidden_since: Some(now),
            asleep: false,
        }
    }
}

#[derive(Default)]
struct State {
    slots: HashMap<Key, Slot>,
    /// Each surface's pass count ([`Surface::index`]).
    passes: [u64; 2],
    /// The view each surface renders in, redrawn when a sleeping slot
    /// gets a new player.
    views: [Option<EntityId>; 2],
    /// The window is in the background or something covers the chat.
    held: bool,
    /// The window's display scale; 0 until the first frame.
    scale: f32,
    /// Clips that failed to open or decode: their still stays.
    failed: HashSet<FileId>,
    timer: Option<Task<()>>,
    /// A sweep is deferred to the end of this frame: the surfaces that
    /// render in it share it, so a second sweep cannot find the first
    /// one's marks unpainted.
    sweep_due: bool,
    /// The history list rendered since the last app frame began.
    history_listed: bool,
}

impl State {
    fn awake(&self) -> Vec<(Key, Option<Instant>)> {
        self.slots
            .iter()
            .filter(|(_, slot)| !slot.asleep)
            .map(|(key, slot)| (*key, slot.hidden_since))
            .collect()
    }
}

/// The players (see the module docs), shared with the layers that draw
/// them.
#[derive(Default)]
pub(super) struct UserpicVideos {
    state: RefCell<State>,
}

/// A play or pause to make once the state's borrow is released.
fn settle(commands: Vec<(VideoPlayer, bool)>) {
    for (player, play) in commands {
        if player.state() == PlayState::Failed {
            continue;
        }
        if play && !player.wants_play() {
            player.play();
        } else if !play && player.wants_play() {
            player.pause();
        }
    }
}

impl UserpicVideos {
    /// A short borrow of the state; `None` when it is already borrowed
    /// further up the stack (never expected: the call is skipped).
    fn with<R>(&self, f: impl FnOnce(&mut State) -> R) -> Option<R> {
        let mut state = self.state.try_borrow_mut().ok()?;
        Some(f(&mut state))
    }

    /// Start a pass of `surface` (its list renders): the slots whose rows
    /// did not render in its last pass go, and a sweep runs once the pass
    /// has painted.
    pub(super) fn begin_render(
        self: &Rc<Self>,
        surface: Surface,
        view: Option<EntityId>,
        cx: &mut App,
    ) {
        let dropped = self.with(|state| {
            let ix = surface.index();
            let pass = state.passes.get(ix).copied().unwrap_or(0).saturating_add(1);
            if let Some(slot) = state.passes.get_mut(ix) {
                *slot = pass;
            }
            if let Some(slot) = state.views.get_mut(ix) {
                *slot = view;
            }
            state.history_listed |= surface == Surface::History;
            let (keep, drop): (HashMap<Key, Slot>, HashMap<Key, Slot>) =
                std::mem::take(&mut state.slots)
                    .into_iter()
                    .partition(|((on, _), slot)| *on != surface || survives(slot.seen, pass));
            state.slots = keep;
            (drop, !std::mem::replace(&mut state.sweep_due, true))
        });
        let Some((dropped, defer)) = dropped else {
            return;
        };
        drop(dropped);
        if defer {
            let videos = self.clone();
            cx.defer(move |cx| {
                videos.with(|state| state.sweep_due = false);
                videos.sweep(true, cx);
            });
        }
    }

    /// At the start of every app frame: `held` (background window, or
    /// something over the chat) pauses every player and lifts on return;
    /// a new display scale restarts them at the new size. When the last
    /// frame rendered a conversation (`history_drawn`) without its message
    /// list (an empty chat), the history's players go.
    pub(super) fn frame_start(&self, held: bool, scale: f32, history_drawn: bool) {
        let Some((commands, dropped)) = self.with(|state| {
            let mut dropped = HashMap::new();
            if history_drawn && !state.history_listed {
                let (history, rest): (HashMap<Key, Slot>, HashMap<Key, Slot>) =
                    std::mem::take(&mut state.slots)
                        .into_iter()
                        .partition(|((on, _), _)| *on == Surface::History);
                state.slots = rest;
                dropped = history;
            }
            state.history_listed = false;
            if state.scale > 0. && (state.scale - scale).abs() > f32::EPSILON {
                dropped.extend(std::mem::take(&mut state.slots));
            }
            state.scale = scale;
            if state.held == held {
                return (Vec::new(), dropped);
            }
            state.held = held;
            let commands = state
                .slots
                .values()
                .filter(|slot| !slot.asleep)
                .map(|slot| (slot.player.clone(), !held && slot.hidden_since.is_none()))
                .collect();
            (commands, dropped)
        }) else {
            return;
        };
        drop(dropped);
        settle(commands);
    }

    /// `key`'s player for its userpic, opened on first use (`path` is only
    /// resolved then) to decode at `size` pt; its row is rendering. `None`
    /// keeps the still: the clip failed, or every allowed player plays on
    /// screen.
    pub(super) fn frame(
        &self,
        key: Key,
        clip: FileId,
        size: f32,
        path: impl FnOnce() -> Option<PathBuf>,
        cx: &mut App,
    ) -> Option<VideoPlayer> {
        let (current, failed, scale, pass) = self.with(|state| {
            let current = state
                .slots
                .get(&key)
                .filter(|slot| slot.clip == clip)
                .map(|slot| slot.player.clone());
            (
                current,
                state.failed.contains(&clip),
                state.scale,
                state.passes.get(key.0.index()).copied().unwrap_or(0),
            )
        })?;
        if failed {
            return None;
        }
        let player = match current {
            Some(player) if player.state() == PlayState::Failed => {
                let dropped = self.with(|state| {
                    state.failed.insert(clip);
                    state.slots.remove(&key)
                });
                drop(dropped);
                return None;
            }
            Some(player) => player,
            None => self.open(key, clip, decode_edge(size, scale), path, cx)?,
        };
        self.with(|state| {
            if let Some(slot) = state.slots.get_mut(&key) {
                slot.seen = pass;
            }
        });
        Some(player)
    }

    /// The current player of a playing userpic, for its layer.
    pub(super) fn current(&self, key: Key) -> Option<VideoPlayer> {
        self.with(|state| state.slots.get(&key).map(|slot| slot.player.clone()))?
    }

    fn open(
        &self,
        key: Key,
        clip: FileId,
        max_edge: u32,
        path: impl FnOnce() -> Option<PathBuf>,
        cx: &mut App,
    ) -> Option<VideoPlayer> {
        let path = path()?;
        let admit = self.with(|state| {
            let mut awake = state.awake();
            awake.retain(|(other, _)| *other != key);
            admit(&awake, MAX_AWAKE)
        })?;
        let evicted = match admit {
            Admit::Refuse => return None,
            Admit::Open => None,
            Admit::Evict(other) => self.with(|state| {
                let slot = state.slots.get_mut(&other)?;
                slot.asleep = true;
                slot.hidden_since = None;
                Some(slot.player.clone())
            })?,
        };
        if let Some(player) = evicted {
            player.hibernate();
        }
        let player = match VideoPlayer::open(&path, Purpose::Inline { max_edge }, None, cx) {
            Ok(player) => player,
            Err(_) => {
                self.with(|state| state.failed.insert(clip));
                return None;
            }
        };
        player.set_volume(0.0);
        player.set_looping(true);
        let slot = Slot::held_until_painted(
            player.clone(),
            clip,
            path,
            max_edge,
            cx.background_executor().now(),
        );
        let replaced = self.with(|state| {
            let seen = state.passes.get(key.0.index()).copied().unwrap_or(0);
            state.slots.insert(key, Slot { seen, ..slot })
        });
        drop(replaced);
        Some(player)
    }

    /// The off-screen policy: play what was painted in view since the last
    /// sweep, pause what was not, put to sleep what has been off screen
    /// for [`RELEASE_AFTER`], and give a sleeping slot painted in view a
    /// new player. While held nothing plays, and only a sweep after a
    /// surface pass (`painted`) runs: a held player is not redrawn, so the
    /// timer could not tell it from one off screen.
    pub(super) fn sweep(self: &Rc<Self>, painted: bool, cx: &mut App) {
        let now = cx.background_executor().now();
        let Some(players) = self.with(|state| {
            (painted || !state.held).then(|| {
                state
                    .slots
                    .iter()
                    .map(|(key, slot)| (*key, slot.player.clone(), slot.shown_mark))
                    .collect::<Vec<_>>()
            })
        }) else {
            return;
        };
        let Some(players) = players else {
            return;
        };
        let counts: Vec<(Key, u64, u64)> = players
            .into_iter()
            .map(|(key, player, mark)| (key, player.shown_count(), mark))
            .collect();
        let Some((commands, sleepers, revive)) = self.with(|state| {
            let mut commands = Vec::new();
            let mut sleepers = Vec::new();
            let mut revive = Vec::new();
            let held = state.held;
            for (key, count, mark) in counts {
                let Some(slot) = state.slots.get_mut(&key) else {
                    continue;
                };
                slot.shown_mark = count;
                let visible = count > mark;
                if slot.asleep {
                    if visible {
                        revive.push(key);
                    }
                    continue;
                }
                match off_screen_step(visible, slot.hidden_since, now) {
                    OffScreenStep::Show => slot.hidden_since = None,
                    OffScreenStep::Hide => slot.hidden_since = Some(now),
                    OffScreenStep::Stay => {}
                    OffScreenStep::Release => {
                        slot.asleep = true;
                        slot.hidden_since = None;
                        sleepers.push(slot.player.clone());
                        continue;
                    }
                }
                commands.push((slot.player.clone(), !held && slot.hidden_since.is_none()));
            }
            (commands, sleepers, revive)
        }) else {
            return;
        };
        settle(commands);
        for player in sleepers {
            player.hibernate();
        }
        for key in revive {
            self.revive(key, cx);
        }
        self.arm_timer(cx);
    }

    /// A sleeping slot was painted in view: open a new player for it (if
    /// one is allowed) and redraw its surface, whose row still holds the
    /// sleeping one.
    fn revive(&self, key: Key, cx: &mut App) {
        let Some((clip, path, max_edge, view)) = self
            .with(|state| {
                let view = state.views.get(key.0.index()).copied().flatten();
                let slot = state.slots.get(&key)?;
                Some((slot.clip, slot.path.clone(), slot.max_edge, view))
            })
            .flatten()
        else {
            return;
        };
        if self.open(key, clip, max_edge, || Some(path), cx).is_some()
            && let Some(view) = view
        {
            cx.notify(view);
        }
    }

    /// Sweep again in [`RELEASE_AFTER`] while a player is awake, so one
    /// whose surface stopped drawing pauses and sleeps.
    fn arm_timer(self: &Rc<Self>, cx: &mut App) {
        let wanted = self
            .with(|state| {
                state.timer.is_none()
                    && !state.held
                    && state.slots.values().any(|slot| !slot.asleep)
            })
            .unwrap_or(false);
        if !wanted {
            return;
        }
        let weak = Rc::downgrade(self);
        let timer = cx.spawn(async move |cx| {
            cx.background_executor().timer(RELEASE_AFTER).await;
            let Some(videos) = weak.upgrade() else {
                return;
            };
            videos.with(|state| state.timer = None);
            cx.update(|cx| videos.sweep(false, cx));
        });
        self.with(|state| state.timer = Some(timer));
    }

    /// Whether the slot plays now, for tests.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn is_playing(&self, key: Key) -> bool {
        self.with(|state| {
            state
                .slots
                .get(&key)
                .is_some_and(|slot| !state.held && !slot.asleep && slot.player.wants_play())
        })
        .unwrap_or(false)
    }

    /// The slot's state, for tests: `None` without one, else whether it
    /// sleeps.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn asleep(&self, key: Key) -> Option<bool> {
        self.with(|state| state.slots.get(&key).map(|slot| slot.asleep))
            .flatten()
    }

    /// The keys with a slot, for tests.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn keys(&self) -> Vec<Key> {
        self.with(|state| state.slots.keys().copied().collect())
            .unwrap_or_default()
    }
}

/// A userpic's playing clip, as its row renders it: the layer redraws it
/// from the pool's current player.
#[derive(Clone)]
pub(crate) struct UserpicFrame {
    videos: Rc<UserpicVideos>,
    key: Key,
    player: VideoPlayer,
    /// The color behind the circle, painted over a GPU surface's corners.
    behind: Hsla,
}

impl UserpicFrame {
    pub(super) fn new(
        videos: Rc<UserpicVideos>,
        key: Key,
        player: VideoPlayer,
        behind: Hsla,
    ) -> Self {
        Self {
            videos,
            key,
            player,
            behind,
        }
    }

    fn picture(player: &VideoPlayer, size: f32, behind: Hsla) -> AnyElement {
        div()
            .size(px(size))
            .child(
                video_frame(player)
                    .object_fit(ObjectFit::Cover)
                    .corner_radii(Corners::all(px(size / 2.)), behind),
            )
            .into_any_element()
    }

    /// `still` with the clip over it, `size` pt square.
    pub(in crate::ui) fn over(this: Option<&Self>, still: AnyElement, size: f32) -> AnyElement {
        let Some(this) = this else {
            return still;
        };
        let (videos, key, behind) = (this.videos.clone(), this.key, this.behind);
        let tile =
            super::anim_layer::tile(Self::picture(&this.player, size, behind), 0, move || {
                videos
                    .current(key)
                    .map(|player| Self::picture(&player, size, behind))
            });
        div()
            .relative()
            .flex_none()
            .size(px(size))
            .child(still)
            .child(div().absolute().top_0().left_0().size(px(size)).child(tile))
            .into_any_element()
    }
}

impl std::fmt::Debug for UserpicFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserpicFrame")
            .field("key", &self.key)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{Admit, MAX_AWAKE, admit, decode_edge, survives};
    use std::time::{Duration, Instant};

    #[test]
    fn userpics_decode_at_their_size_in_device_pixels() {
        assert_eq!(decode_edge(32., 1.), 32);
        assert_eq!(decode_edge(32., 2.), 64);
        assert_eq!(decode_edge(46., 2.), 92);
        assert_eq!(decode_edge(46., 1.5), 69);
        assert_eq!(decode_edge(32., 0.), 64);
        assert_eq!(decode_edge(32., f32::NAN), 64);
        assert_eq!(decode_edge(400., 2.), 320);
        assert_eq!(decode_edge(2., 1.), 16);
    }

    #[test]
    fn a_slot_lives_while_its_row_renders() {
        assert!(survives(4, 5));
        assert!(survives(5, 5));
        assert!(!survives(3, 5));
        assert!(survives(u64::MAX, 1));
    }

    #[test]
    fn the_pool_opens_until_full_then_evicts_the_longest_off_screen() {
        let now = Instant::now();
        let ago = |ms| now.checked_sub(Duration::from_millis(ms)).unwrap();
        assert_eq!(admit::<u8>(&[], MAX_AWAKE), Admit::Open);
        assert_eq!(admit(&[(1u8, None), (2, Some(ago(10)))], 3), Admit::Open);
        assert_eq!(
            admit(&[(1u8, Some(ago(10))), (2, None), (3, Some(ago(500)))], 3),
            Admit::Evict(3)
        );
        assert_eq!(admit(&[(1u8, None), (2, None)], 2), Admit::Refuse);
    }
}
