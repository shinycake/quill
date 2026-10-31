//! The inline players of the history and the GIF panel: one muted, looping
//! [`VideoPlayer`] (`Purpose::Inline`) per clip on screen, keyed by
//! message, under one decoded-memory budget.
//!
//! - A player starts the first time its row renders a downloaded clip,
//!   and is dropped once no window shows it: a clip leaves a window once
//!   its row has not rendered for a whole pass of that window's history
//!   (scrolled away, chat changed, viewer open) or the window closes
//!   ([`windows`]). A chat window's clips play while the main window
//!   draws another chat.
//! - Off-screen policy: after each history pass the players whose picture
//!   was not painted inside the visible area in that pass (nor in the
//!   last pass of another window showing them that the platform presents)
//!   are paused at once (no more decoding, no more frame requests),
//!   resumed when a sweep sees them painted in view again, and put to
//!   sleep after [`RELEASE_AFTER`] off screen ([`off_screen_step`]): the
//!   decoder goes, the slot stays with a backend-less player that paints
//!   nothing over the still and only counts being painted, so a row that
//!   keeps rendering in the list's overdraw does not reopen its decoder
//!   every pass. A sweep that sees a sleeping slot painted in view opens a new
//!   player for it (`revive`). tdesktop pauses a clip that is not drawn
//!   the same way (its streamed player only decodes ahead of frames shown)
//!   and unloads it once far from the visible area (`unloadHeavyViewParts`).
//! - A round video message plays once with sound when clicked, then loops
//!   muted again.
//! - With autoplay off (or data saver on) a clip plays only on demand
//!   ([`Demand`]): a clicked round video once with sound, a clicked MP4
//!   GIF as a muted loop. The demand lasts until the round video ends or
//!   the slot is dropped.
//!
//! The state sits in one `RefCell`; every borrow is one short scope, and
//! no player call (each reaches the backend) is made while it is held.
#![deny(clippy::indexing_slicing)]

mod demand;
mod windows;

use super::inline_video::{InlineFrame, InlineTile};
use super::video_player::{PlayState, Purpose, VideoPlayer};
pub(super) use demand::Demand;
use gpui_kit::*;
use quill::ids::ChatId;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};
use windows::{List, Seen, Sweep, Windows, note_sweep};

/// `(chat id, message id)`.
pub(super) type Key = (i64, i64);

/// A panel over the composer whose clips play while it shows: the inline
/// results or the media panel's GIFs. Its players live under a chat id no
/// chat uses; the history's off-screen sweep leaves them alone, and each
/// panel's render passes reap only its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Panel {
    InlineResults,
    Gifs,
}

impl Panel {
    pub(super) const fn chat(self) -> i64 {
        match self {
            Self::InlineResults => i64::MIN,
            Self::Gifs => i64::MIN + 1,
        }
    }

    fn of(chat: i64) -> Option<Self> {
        [Self::InlineResults, Self::Gifs]
            .into_iter()
            .find(|panel| panel.chat() == chat)
    }
}

fn is_panel(chat: i64) -> bool {
    Panel::of(chat).is_some()
}

/// Decoded picture memory (queued pictures plus the one shown) all inline
/// players together may hold. Past it a further tile keeps its still until
/// a running clip stops; with tiles sized to the screen it takes a dozen
/// or more large clips on screen at once to get there.
pub(super) const INLINE_DECODE_BUDGET: usize = 64 * 1024 * 1024;

/// An off-screen player is put to sleep (its decoder and budget freed)
/// after this long, even when no history pass dropped it.
pub(super) const RELEASE_AFTER: Duration = Duration::from_secs(2);

/// tdesktop `VideoMessageSeek`: the ring shows in 220 ms (ease-out-back),
/// hides in 150 ms, and the dot grows or shrinks in 150 ms.
const SEEK_SHOW: f32 = 0.22;
const SEEK_HIDE: f32 = 0.15;
const SEEK_GRAB: f32 = 0.15;

/// Whether a new player holding `more` bytes fits next to players holding
/// `held` ([`INLINE_DECODE_BUDGET`]). The first one always does.
pub(super) fn admits(held: usize, more: usize) -> bool {
    held == 0 || held.saturating_add(more) <= INLINE_DECODE_BUDGET
}

/// What the off-screen sweep does with one player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OffScreenStep {
    /// Painted in view: play (or keep playing).
    Show,
    /// Not painted in view this pass: pause from now on.
    Hide,
    /// Still off screen, not long enough to release.
    Stay,
    /// Off screen for [`RELEASE_AFTER`]: drop the decoder (sleep).
    Release,
}

/// The off-screen policy for one player: `visible` (its picture was
/// painted inside the visible area since the pass began), since when it is
/// hidden, and the time now.
pub(super) fn off_screen_step(
    visible: bool,
    hidden_since: Option<Instant>,
    now: Instant,
) -> OffScreenStep {
    match (visible, hidden_since) {
        (true, _) => OffScreenStep::Show,
        (false, None) => OffScreenStep::Hide,
        (false, Some(since)) if now.saturating_duration_since(since) >= RELEASE_AFTER => {
            OffScreenStep::Release
        }
        (false, Some(_)) => OffScreenStep::Stay,
    }
}

/// `anim::easeOutBack`: overshoots a little, then settles.
fn ease_out_back(t: f32) -> f32 {
    const S: f32 = 1.70158;
    let t = t - 1.0;
    t * t * ((S + 1.0) * t + S) + 1.0
}

/// A value that eases between 0 and 1 when its target flips.
#[derive(Clone, Copy)]
struct Toggle {
    on: bool,
    since: Instant,
    from: f32,
}

impl Toggle {
    fn new() -> Self {
        Self {
            on: false,
            since: Instant::now(),
            from: 0.0,
        }
    }

    /// Point it at `on`, starting from wherever it is now.
    fn set(&mut self, on: bool, show: f32, hide: f32, spring: bool) {
        if self.on != on {
            self.from = self.value(show, hide, spring).clamp(0.0, 1.0);
            self.on = on;
            self.since = Instant::now();
        }
    }

    fn value(&self, show: f32, hide: f32, spring: bool) -> f32 {
        let elapsed = self.since.elapsed().as_secs_f32();
        if self.on {
            let t = (elapsed / show).min(1.0);
            let eased = if spring { ease_out_back(t) } else { t };
            self.from + (1.0 - self.from) * eased
        } else {
            let t = (elapsed / hide).min(1.0);
            self.from * (1.0 - t)
        }
    }

    fn settled(&self, show: f32, hide: f32) -> bool {
        self.since.elapsed().as_secs_f32() >= if self.on { show } else { hide }
    }
}

/// A call to make on a player once the state's borrow is released.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Command {
    Play,
    Pause,
    /// Leave the play-once-with-sound mode: muted and looping, playing
    /// only when the slot wants it (on screen, window active).
    BackToLoop {
        play: bool,
    },
    /// Start playing once with sound from the start.
    PlayWithSound,
    Seek(f64),
}

fn run(commands: Vec<(VideoPlayer, Command)>) {
    for (player, command) in commands {
        match command {
            Command::Play => player.play(),
            Command::Pause => player.pause(),
            Command::BackToLoop { play } => {
                player.set_volume(0.0);
                player.set_looping(true);
                if play {
                    player.play();
                } else {
                    player.pause();
                }
            }
            Command::PlayWithSound => {
                player.set_looping(false);
                player.set_volume(1.0);
                player.seek(0.0);
                player.play();
            }
            Command::Seek(secs) => player.seek(secs),
        }
    }
}

/// What a new player for the slot is opened from.
#[derive(Clone)]
struct Source {
    path: PathBuf,
    max_edge: u32,
    /// Decoded picture memory a player of it holds (`Slot::bytes`).
    bytes: usize,
    /// The message's duration, shown until the file's own is known.
    duration: Option<f64>,
}

struct Slot {
    player: VideoPlayer,
    source: Source,
    /// Decoded picture memory it may hold (0 for AVPlayer, whose buffers
    /// aren't ours, and for a sleeping slot).
    bytes: usize,
    /// Off screen for [`RELEASE_AFTER`]: the player has no backend
    /// (`VideoPlayer::hibernate`) until a sweep sees it painted in view.
    asleep: bool,
    /// The windows its row renders in, each with the pass it last
    /// rendered in there (the history's, or its panel's for a [`Panel`]
    /// key), the paint count when that pass began, and whether that
    /// window's last sweep found it in view.
    windows: Windows,
    /// Off screen since then: paused, released after [`RELEASE_AFTER`].
    hidden_since: Option<Instant>,
    sound: bool,
    /// Paused by a click while playing with sound.
    paused: bool,
    /// Dragging along the seek ring: whether it was playing before.
    seeking: Option<bool>,
    seek_shown: Toggle,
    seek_grabbed: Toggle,
}

impl Slot {
    /// Whether the player should be running now.
    fn wants_play(&self, inactive: bool) -> bool {
        !(self.paused
            || self.seeking.is_some()
            // A clip playing with sound keeps going off screen, as in
            // tdesktop (it hands it to the floating player); only muted
            // loops hold still there.
            || (self.hidden_since.is_some() && !self.sound)
            // Behind another app muted loops hold still (tdesktop pauses
            // GIFs and round loops there).
            || (inactive && !self.sound))
    }

    /// What it takes to bring the player in line with the slot, given its
    /// transport state and whether play is asked of it (`running`, which
    /// an opening player records too: it starts by itself once open).
    fn settle(&mut self, state: PlayState, running: bool, inactive: bool) -> Option<Command> {
        // Played once with sound and done: back to the muted loop.
        if self.sound && state == PlayState::Ended {
            self.sound = false;
            self.paused = false;
            return Some(Command::BackToLoop {
                play: self.wants_play(inactive),
            });
        }
        if state == PlayState::Failed {
            return None;
        }
        match (self.wants_play(inactive), running) {
            (true, false) => Some(Command::Play),
            (false, true) => Some(Command::Pause),
            _ => None,
        }
    }

    fn frame(&mut self, held_ring: bool) -> InlineFrame {
        self.seek_shown
            .set(self.sound && held_ring, SEEK_SHOW, SEEK_HIDE, true);
        self.seek_grabbed
            .set(self.seeking.is_some(), SEEK_GRAB, SEEK_GRAB, false);
        let transport = self.player.transport();
        let remaining_secs = transport
            .duration
            .map(|total| (total - transport.position).max(0.0));
        let progress = transport
            .duration
            .filter(|total| *total > 0.0)
            .map(|total| (transport.position / total).clamp(0.0, 1.0) as f32);
        InlineFrame {
            player: self.player.clone(),
            remaining_secs,
            sound: self.sound,
            backdrop: gpui_kit::black(),
            progress,
            seek_shown: self.seek_shown.value(SEEK_SHOW, SEEK_HIDE, true),
            seek_grabbed: self.seek_grabbed.value(SEEK_GRAB, SEEK_GRAB, false),
            live: None,
        }
    }
}

#[derive(Default)]
struct State {
    players: HashMap<Key, Slot>,
    /// Each window's pass counts, of its history and of each panel
    /// (`None`: the main window's).
    passes: HashMap<(Option<ChatId>, List), u64>,
    /// The view each window's history renders in (`begin_render`): redrawn
    /// once a sleeping slot it shows gets a new player, so its tile picks
    /// it up.
    views: HashMap<Option<ChatId>, EntityId>,
    /// The windows whose history rendered (and swept the players) since
    /// their last frame began.
    swept: HashSet<Option<ChatId>>,
    /// The window is in the background: muted loops hold still.
    inactive: bool,
    /// The window's display scale (device pixels per point); 0 until the
    /// first frame reports it.
    scale: f32,
    /// The timer that releases players off screen for [`RELEASE_AFTER`].
    release_timer: Option<Task<()>>,
    /// Clips asked for by a click (autoplay off, data saver, or not
    /// downloaded yet).
    on_demand: HashMap<Key, demand::OnDemand>,
    /// The windows whose presentation is watched (from their first frame
    /// until they close).
    watched: HashMap<Option<ChatId>, Subscription>,
    /// The watched windows the platform does not present now.
    unpresented: HashSet<Option<ChatId>>,
    /// The active chat window hides the main window whole
    /// (`chat_window::cover`): presented, it draws nothing.
    main_covered: bool,
}

/// The players (see the module docs). Shared (`Rc`) with the history's
/// animation layer, which draws their pictures.
#[derive(Default)]
pub(super) struct InlineVideos {
    state: RefCell<State>,
}

impl InlineVideos {
    /// A short borrow of the state; `None` when it is already borrowed
    /// further up the stack (never expected: the call is skipped, and
    /// `QUILL_TRACE_VIDEO=1` logs it).
    #[track_caller]
    fn with<R>(&self, f: impl FnOnce(&mut State) -> R) -> Option<R> {
        let Ok(mut state) = self.state.try_borrow_mut() else {
            if super::video_player::trace_video() {
                eprintln!(
                    "inline players: re-entrant borrow skipped at {}",
                    std::panic::Location::caller()
                );
            }
            return None;
        };
        Some(f(&mut state))
    }

    /// Start a pass of `window`'s history (`None`: the main window's),
    /// rendered in `view`: its clips whose rows didn't render in its last
    /// pass leave it (and stop once no window shows them), and the paint
    /// count of each clip it shows is noted. Once the pass has painted
    /// (deferred to the end of this frame), its clips not painted in view
    /// are paused ([`Self::sweep`]).
    pub(super) fn begin_render(
        self: &Rc<Self>,
        window: Option<ChatId>,
        view: Option<EntityId>,
        cx: &mut App,
    ) {
        let dropped = self.with(|state| {
            state.swept.insert(window);
            match view {
                Some(view) => state.views.insert(window, view),
                None => state.views.remove(&window),
            };
            state.begin_pass(window, List::History)
        });
        let marks: Vec<(Key, VideoPlayer)> = self
            .with(|state| {
                state
                    .players
                    .iter()
                    .filter(|(_, slot)| slot.windows.contains_key(&window))
                    .map(|(key, slot)| (*key, slot.player.clone()))
                    .collect()
            })
            .unwrap_or_default();
        // Read outside the borrow (the counts are the players' own).
        let marks: Vec<(Key, u64)> = marks
            .into_iter()
            .map(|(key, player)| (key, player.shown_count()))
            .collect();
        self.with(|state| {
            for (key, mark) in marks {
                if let Some(seen) = state
                    .players
                    .get_mut(&key)
                    .and_then(|slot| slot.windows.get_mut(&window))
                {
                    seen.mark = mark;
                }
            }
        });
        drop(dropped);
        let videos = self.clone();
        cx.defer(move |cx| videos.sweep(Sweep::Pass(window), cx));
    }

    /// The off-screen policy after a window's history pass painted (for
    /// the clips it shows), when the release timer fires, or when a window
    /// is hidden or presented again: pause what no window has in view,
    /// resume what one has, put to sleep what has been off screen for
    /// [`RELEASE_AFTER`], and give a sleeping slot painted in view a new
    /// player.
    fn sweep(self: &Rc<Self>, sweep: Sweep, cx: &mut App) {
        let now = cx.background_executor().now();
        let players: Vec<(Key, VideoPlayer)> = self
            .with(|state| {
                state
                    .players
                    .iter()
                    .filter(|((chat, _), slot)| {
                        !is_panel(*chat)
                            && match sweep {
                                Sweep::Pass(window) => slot.windows.contains_key(&window),
                                Sweep::Timer | Sweep::Recheck => true,
                            }
                    })
                    .map(|(key, slot)| (*key, slot.player.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let counted: Vec<(Key, u64, PlayState, bool)> = players
            .into_iter()
            .map(|(key, player)| {
                (
                    key,
                    player.shown_count(),
                    player.state(),
                    player.wants_play(),
                )
            })
            .collect();
        let Some((commands, sleepers, revive, hidden)) = self.with(|state| {
            let inactive = state.inactive;
            let unseen = state.unseen();
            let mut commands = Vec::new();
            let mut sleepers = Vec::new();
            let mut revive = Vec::new();
            for (key, count, play_state, running) in counted {
                let Some(slot) = state.players.get_mut(&key) else {
                    continue;
                };
                let visible = note_sweep(&mut slot.windows, sweep, count, &unseen);
                if slot.asleep {
                    if visible {
                        revive.push((key, slot.source.clone()));
                    }
                    continue;
                }
                match off_screen_step(visible, slot.hidden_since, now) {
                    OffScreenStep::Show => slot.hidden_since = None,
                    OffScreenStep::Hide => slot.hidden_since = Some(now),
                    OffScreenStep::Stay => {}
                    // Never cut a clip short while it is heard.
                    OffScreenStep::Release if slot.sound => {}
                    OffScreenStep::Release => {
                        slot.asleep = true;
                        slot.bytes = 0;
                        slot.hidden_since = None;
                        sleepers.push(slot.player.clone());
                        continue;
                    }
                }
                if let Some(command) = slot.settle(play_state, running, inactive) {
                    commands.push((slot.player.clone(), command));
                }
            }
            let hidden = state
                .players
                .values()
                .any(|slot| slot.hidden_since.is_some());
            (commands, sleepers, revive, hidden)
        }) else {
            return;
        };
        run(commands);
        for player in sleepers {
            player.hibernate();
        }
        for (key, source) in revive {
            self.revive(key, source, cx);
        }
        if hidden {
            self.arm_release(cx);
        }
    }

    /// A sleeping slot was painted in view: open a new player for it (if
    /// the budget admits one) and redraw the histories showing it, whose
    /// tiles still hold the sleeping one.
    fn revive(&self, key: Key, source: Source, cx: &mut App) {
        let Some((held, inactive, views)) = self.with(|state| {
            let views: Vec<EntityId> = state.players.get(&key).map_or(Vec::new(), |slot| {
                slot.windows
                    .keys()
                    .filter_map(|window| state.views.get(window).copied())
                    .collect()
            });
            (
                state.players.values().map(|slot| slot.bytes).sum::<usize>(),
                state.inactive,
                views,
            )
        }) else {
            return;
        };
        if !admits(held, source.bytes) {
            return;
        }
        let Ok(player) = VideoPlayer::open(
            &source.path,
            Purpose::Inline {
                max_edge: source.max_edge,
            },
            source.duration,
            cx,
        ) else {
            return;
        };
        player.set_volume(0.0);
        player.set_looping(true);
        // It is in view: play, unless the window is behind another app.
        if !inactive {
            player.play();
        }
        let mark = player.shown_count();
        let replaced = self.with(|state| {
            let slot = state.players.get_mut(&key)?;
            slot.asleep = false;
            slot.bytes = source.bytes;
            for seen in slot.windows.values_mut() {
                seen.mark = mark;
            }
            slot.hidden_since = None;
            Some(std::mem::replace(&mut slot.player, player))
        });
        drop(replaced);
        for view in views {
            cx.notify(view);
        }
    }

    /// Sweep again once an off-screen player is due for release.
    fn arm_release(self: &Rc<Self>, cx: &mut App) {
        if self
            .with(|state| state.release_timer.is_some())
            .unwrap_or(true)
        {
            return;
        }
        let weak = Rc::downgrade(self);
        let timer = cx.spawn(async move |cx| {
            cx.background_executor().timer(RELEASE_AFTER).await;
            let Some(videos) = weak.upgrade() else {
                return;
            };
            videos.with(|state| state.release_timer = None);
            cx.update(|cx| videos.sweep(Sweep::Timer, cx));
        });
        self.with(|state| state.release_timer = Some(timer));
    }

    /// The main window's display scale (device pixels per point), at the
    /// start of each of its frames, for the size new players decode at.
    pub(super) fn set_scale(&self, scale: f32) {
        let dropped = self.with(|state| {
            let mut dropped = HashMap::new();
            // The display or interface scale changed: running players
            // decode at the old size, so restart them at the new one.
            if state.scale > 0. && (state.scale - scale).abs() > f32::EPSILON {
                dropped = std::mem::take(&mut state.players);
                state.forget(&dropped);
            }
            state.scale = scale;
            dropped
        });
        drop(dropped);
    }

    /// The window became active or inactive: muted loops pause behind
    /// another app and resume on return; a clip playing with sound keeps
    /// playing.
    pub(super) fn set_window_active(&self, active: bool) {
        let commands = self
            .with(|state| {
                state.inactive = !active;
                state
                    .players
                    .values()
                    .map(|slot| (slot.player.clone(), slot.wants_play(!active)))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (player, wants) in commands {
            // Play and pause on an opening player only record the intent.
            if player.state() == PlayState::Failed {
                continue;
            }
            let running = player.wants_play();
            if wants && !running {
                player.play();
            } else if !wants && running {
                player.pause();
            }
        }
    }

    /// Stop everything (viewer opened, autoplay turned off).
    pub(super) fn clear(&self) {
        let dropped = self.with(|state| {
            state.on_demand.clear();
            std::mem::take(&mut state.players)
        });
        drop(dropped);
    }

    /// A click on a running clip (tdesktop's round video message): a
    /// muted loop plays once from the start with sound (muting any other),
    /// a sounding one pauses, a paused one resumes. False when it isn't
    /// running.
    pub(super) fn toggle_sound(&self, chat_id: i64, message_id: i64) -> bool {
        let key = (chat_id, message_id);
        let Some(commands) = self.with(|state| {
            let slot = state.players.get_mut(&key)?;
            if slot.sound {
                slot.paused = !slot.paused;
                let command = if slot.paused {
                    Command::Pause
                } else {
                    Command::Play
                };
                return Some(vec![(slot.player.clone(), command)]);
            }
            let mut commands = Vec::new();
            let inactive = state.inactive;
            for (other, slot) in &mut state.players {
                if *other != key && slot.sound {
                    slot.sound = false;
                    slot.paused = false;
                    let play = slot.wants_play(inactive);
                    commands.push((slot.player.clone(), Command::BackToLoop { play }));
                }
            }
            if let Some(slot) = state.players.get_mut(&key) {
                slot.sound = true;
                slot.paused = false;
                commands.push((slot.player.clone(), Command::PlayWithSound));
            }
            Some(commands)
        }) else {
            return false;
        };
        let Some(commands) = commands else {
            return false;
        };
        run(commands);
        true
    }

    /// Drag along a sounding clip's seek ring to `fraction` of it: the
    /// clip holds still while dragged and resumes on release if it was
    /// playing.
    pub(super) fn seek_to(&self, chat_id: i64, message_id: i64, fraction: f32) -> bool {
        let commands = self.with(|state| {
            let slot = state.players.get_mut(&(chat_id, message_id))?;
            if !slot.sound {
                return None;
            }
            let mut commands = Vec::new();
            if slot.seeking.is_none() {
                slot.seeking = Some(!slot.paused);
                commands.push((slot.player.clone(), Command::Pause));
            }
            Some((slot.player.clone(), commands))
        });
        let Some((player, mut commands)) = commands.flatten() else {
            return false;
        };
        if let Some(total) = player.transport().duration {
            commands.push((
                player,
                Command::Seek(f64::from(fraction.clamp(0.0, 1.0)) * total),
            ));
        }
        run(commands);
        true
    }

    pub(super) fn end_seek(&self, chat_id: i64, message_id: i64) {
        let command = self.with(|state| {
            let slot = state.players.get_mut(&(chat_id, message_id))?;
            let was_playing = slot.seeking.take()?;
            slot.paused = !was_playing;
            was_playing.then(|| (slot.player.clone(), Command::Play))
        });
        if let Some(command) = command.flatten() {
            run(vec![command]);
        }
    }

    pub(super) fn is_seeking(&self, chat_id: i64, message_id: i64) -> bool {
        self.with(|state| {
            state
                .players
                .get(&(chat_id, message_id))
                .is_some_and(|slot| slot.seeking.is_some())
        })
        .unwrap_or(false)
    }

    /// Whether a seek ring is still springing in or out.
    pub(super) fn seek_animating(&self) -> bool {
        self.with(|state| {
            state.players.values().any(|slot| {
                !slot.seek_shown.settled(SEEK_SHOW, SEEK_HIDE)
                    || !slot.seek_grabbed.settled(SEEK_GRAB, SEEK_GRAB)
            })
        })
        .unwrap_or(false)
    }

    /// This message's clip, starting its muted, looping player on first
    /// use (`path` is only resolved then), sized for `tile`; its row is
    /// rendering in `window` (`None`: the main window), which shows it from
    /// now on. A player paused off screen stays paused until a sweep
    /// sees it painted in view (a paused player still paints its last
    /// picture, so scrolling back resumes it). A clip that would
    /// take the decoded memory past [`INLINE_DECODE_BUDGET`] doesn't start
    /// (its still shows). `duration` (the message's) shows until the
    /// file's own is known.
    pub(super) fn frame(
        &self,
        window: Option<ChatId>,
        key: Key,
        tile: InlineTile,
        duration: Option<f64>,
        path: impl FnOnce() -> Option<PathBuf>,
        cx: &mut App,
    ) -> Option<InlineFrame> {
        let exists = self.with(|state| state.players.contains_key(&key))?;
        if !exists {
            self.open(window, key, tile, duration, path, cx)?;
        }
        let player = self.with(|state| {
            let pass = state.pass(window, List::of(key));
            let slot = state.players.get_mut(&key)?;
            // A window that just began showing it counts its paints from
            // now (a `Cell` read; no backend call).
            let mark = slot.player.shown_count();
            slot.windows
                .entry(window)
                .or_insert(Seen {
                    pass,
                    in_view: false,
                    mark,
                })
                .pass = pass;
            Some(slot.player.clone())
        })??;
        self.settled_frame(key, &player)
    }

    /// The current frame of a running clip, for the history's animation
    /// layer: [`Self::frame`] without starting a player or marking its row
    /// rendered.
    pub(super) fn current(&self, key: Key) -> Option<InlineFrame> {
        let player =
            self.with(|state| state.players.get(&key).map(|slot| slot.player.clone()))??;
        self.settled_frame(key, &player)
    }

    /// Bring the slot's player in line (loop back, resume, pause) and
    /// return its frame; `None` once its player failed (the still shows).
    fn settled_frame(&self, key: Key, player: &VideoPlayer) -> Option<InlineFrame> {
        let state = player.state();
        if state == PlayState::Failed {
            return None;
        }
        let running = player.wants_play();
        let (command, frame) = self.with(|videos| {
            let inactive = videos.inactive;
            let slot = videos.players.get_mut(&key)?;
            // A sleeping slot's tile paints nothing over the still and
            // only counts being painted; the sweep gives it a player.
            let command = (!slot.asleep)
                .then(|| slot.settle(state, running, inactive))
                .flatten()
                .map(|command| (slot.player.clone(), command));
            let held = slot.paused || slot.seeking.is_some();
            Some((command, slot.frame(held)))
        })??;
        run(command.into_iter().collect());
        Some(frame)
    }

    /// Open the player for `key`, shown in `window`, if the budget admits
    /// it.
    fn open(
        &self,
        window: Option<ChatId>,
        key: Key,
        tile: InlineTile,
        duration: Option<f64>,
        path: impl FnOnce() -> Option<PathBuf>,
        cx: &mut App,
    ) -> Option<()> {
        let (held, scale, inactive, pass) = self.with(|state| {
            (
                state.players.values().map(|slot| slot.bytes).sum::<usize>(),
                state.scale,
                state.inactive,
                state.pass(window, List::of(key)),
            )
        })?;
        let max_edge = tile.decode_edge(scale);
        let bytes = if super::video_player::decodes_in_process() {
            let queued = quill::video_decode::OpenOptions::inline(max_edge).video_frames;
            tile.decode_bytes(max_edge, queued)
        } else {
            0
        };
        if !admits(held, bytes) {
            return None;
        }
        let path = path()?;
        let player = VideoPlayer::open(&path, Purpose::Inline { max_edge }, duration, cx).ok()?;
        player.set_volume(0.0);
        player.set_looping(true);
        // A history clip starts held, as if off screen: the sweep after
        // this pass plays it once its first picture was painted in view,
        // so a row rendered only in the list's overdraw never plays (and
        // one released off screen comes back paused). The GIF panel has no
        // sweep; behind another app a new muted loop waits for activation.
        let in_history = !is_panel(key.0);
        if !inactive && !in_history {
            player.play();
        }
        let hidden_since = in_history.then(|| cx.background_executor().now());
        let mark = player.shown_count();
        let replaced = self.with(|state| {
            state.players.insert(
                key,
                Slot {
                    player,
                    source: Source {
                        path,
                        max_edge,
                        bytes,
                        duration,
                    },
                    bytes,
                    asleep: false,
                    windows: HashMap::from([(
                        window,
                        Seen {
                            pass,
                            in_view: false,
                            mark,
                        },
                    )]),
                    hidden_since,
                    sound: false,
                    paused: false,
                    seeking: None,
                    seek_shown: Toggle::new(),
                    seek_grabbed: Toggle::new(),
                },
            )
        })?;
        drop(replaced);
        Some(())
    }

    /// Whether the clip plays with sound now (a round video played once
    /// with sound, not paused).
    pub(super) fn is_sounding(&self, key: Key) -> bool {
        self.with(|state| {
            state
                .players
                .get(&key)
                .is_some_and(|slot| slot.sound && !slot.paused)
        })
        .unwrap_or(false)
    }

    /// Whether any clip plays with sound now.
    pub(super) fn any_sounding(&self) -> bool {
        self.with(|state| {
            state
                .players
                .values()
                .any(|slot| slot.sound && !slot.paused)
        })
        .unwrap_or(false)
    }

    /// The slot's player, for tests.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn player(&self, key: Key) -> Option<VideoPlayer> {
        self.with(|state| state.players.get(&key).map(|slot| slot.player.clone()))
            .flatten()
    }

    /// Whether the slot is paused or asleep off screen, for tests.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn is_hidden(&self, key: Key) -> bool {
        self.with(|state| {
            state
                .players
                .get(&key)
                .is_some_and(|slot| slot.hidden_since.is_some() || slot.asleep)
        })
        .unwrap_or(false)
    }

    /// Whether the slot sleeps without a decoder, for tests.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn is_asleep(&self, key: Key) -> bool {
        self.with(|state| state.players.get(&key).is_some_and(|slot| slot.asleep))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::{INLINE_DECODE_BUDGET, OffScreenStep, RELEASE_AFTER, admits, off_screen_step};
    use crate::ui::inline_video::InlineTile;
    use std::time::{Duration, Instant};

    #[test]
    fn a_player_painted_in_view_plays() {
        let now = Instant::now();
        assert_eq!(off_screen_step(true, None, now), OffScreenStep::Show);
        // Back in view after a while off screen.
        let since = now.checked_sub(Duration::from_secs(1)).unwrap();
        assert_eq!(off_screen_step(true, Some(since), now), OffScreenStep::Show);
    }

    #[test]
    fn a_player_not_painted_in_the_pass_pauses_at_once() {
        let now = Instant::now();
        // The first pass without a paint: pause in this very frame.
        assert_eq!(off_screen_step(false, None, now), OffScreenStep::Hide);
        // Later passes keep it paused until the release time.
        let since = now.checked_sub(Duration::from_millis(500)).unwrap();
        assert_eq!(
            off_screen_step(false, Some(since), now),
            OffScreenStep::Stay
        );
    }

    #[test]
    fn a_player_off_screen_long_enough_is_released() {
        let now = Instant::now();
        let since = now.checked_sub(RELEASE_AFTER).unwrap();
        assert_eq!(
            off_screen_step(false, Some(since), now),
            OffScreenStep::Release
        );
        let just_before = now
            .checked_sub(RELEASE_AFTER.saturating_sub(Duration::from_millis(1)))
            .unwrap();
        assert_eq!(
            off_screen_step(false, Some(just_before), now),
            OffScreenStep::Stay
        );
    }

    #[test]
    fn players_share_a_decoded_memory_budget() {
        use crate::ui::message_media::MediaFrameKind::Gif;
        // The first clip always plays, however large.
        assert!(admits(0, INLINE_DECODE_BUDGET * 2));
        // Players at 1x 720p-GIF size: dozens fit.
        let each = InlineTile::media(Gif, 1280, 720, 0).decode_bytes(360, 3);
        let fit = (1..).take_while(|n| admits(each * n, each)).count();
        assert!(fit >= 40, "{fit}");
        // At 2x they cost four times as much, still a dozen.
        let each = InlineTile::media(Gif, 1280, 720, 0).decode_bytes(720, 3);
        let fit = (1..).take_while(|n| admits(each * n, each)).count();
        assert!((12..40).contains(&fit), "{fit}");
        // AVPlayer players hold nothing of ours.
        assert!(admits(INLINE_DECODE_BUDGET, 0));
        assert!(!admits(INLINE_DECODE_BUDGET, 1));
    }

    #[test]
    fn the_budget_counts_the_queue_the_inline_decoder_keeps() {
        // `OpenOptions::inline` keeps three pictures ahead of the one shown.
        let queued = quill::video_decode::OpenOptions::inline(320).video_frames;
        assert_eq!(queued, 3);
        let gif = InlineTile::media(crate::ui::message_media::MediaFrameKind::Gif, 1280, 720, 0);
        assert_eq!(gif.decode_bytes(320, queued), 320 * 180 * 4 * 4);
    }
}
