//! The inline players of the history and the GIF panel: one muted, looping
//! [`VideoPlayer`] (`Purpose::Inline`) per clip on screen, keyed by
//! message, under one decoded-memory budget.
//!
//! - A player starts the first time its row renders a downloaded clip,
//!   and is dropped once its row has not rendered for a whole history
//!   pass (scrolled away, chat changed, viewer open).
//! - Off-screen policy: after each history pass the players whose picture
//!   was not painted inside the visible area in that pass are paused at
//!   once (no more decoding, no more frame requests), resumed when their
//!   row renders again, and released after [`RELEASE_AFTER`] off screen
//!   ([`off_screen_step`]). tdesktop pauses a clip that is not drawn the
//!   same way (its streamed player only decodes ahead of frames shown) and
//!   unloads it once far from the visible area (`unloadHeavyViewParts`).
//! - A round video message plays once with sound when clicked, then loops
//!   muted again.
//!
//! The state sits in one `RefCell`; every borrow is one short scope, and
//! no player call (each reaches the backend) is made while it is held.
#![deny(clippy::indexing_slicing)]

use super::inline_video::{InlineFrame, InlineTile};
use super::video_player::{PlayState, Purpose, VideoPlayer};
use gpui_kit::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// `(chat id, message id)`.
pub(super) type Key = (i64, i64);

/// The GIF panel's players live under this chat id, which no chat uses;
/// the off-screen sweep leaves them alone (the panel is not the history).
pub(super) const PANEL_CHAT: i64 = i64::MIN;

/// Decoded picture memory (queued pictures plus the one shown) all inline
/// players together may hold. Past it a further tile keeps its still until
/// a running clip stops; with tiles sized to the screen it takes a dozen
/// or more large clips on screen at once to get there.
pub(super) const INLINE_DECODE_BUDGET: usize = 64 * 1024 * 1024;

/// An off-screen player is released (its decoder and budget freed) after
/// this long, even when no history pass dropped it.
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
    /// Off screen for [`RELEASE_AFTER`]: drop the player.
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
    /// Leave the play-once-with-sound mode: muted, looping, playing.
    BackToLoop,
    /// Start playing once with sound from the start.
    PlayWithSound,
    Seek(f64),
}

fn run(commands: Vec<(VideoPlayer, Command)>) {
    for (player, command) in commands {
        match command {
            Command::Play => player.play(),
            Command::Pause => player.pause(),
            Command::BackToLoop => {
                player.set_volume(0.0);
                player.set_looping(true);
                player.play();
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

struct Slot {
    player: VideoPlayer,
    /// Decoded picture memory it may hold (0 for AVPlayer, whose buffers
    /// aren't ours).
    bytes: usize,
    /// The history pass its row last rendered in.
    seen: u64,
    /// The player's paint count when the current pass began
    /// (`VideoPlayer::shown_count`): more since means it was in view.
    shown_mark: u64,
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
            || self.hidden_since.is_some()
            // Behind another app muted loops hold still (tdesktop pauses
            // GIFs and round loops there).
            || (inactive && !self.sound))
    }

    /// What it takes to bring the player in line with the slot, given its
    /// transport state.
    fn settle(&mut self, state: PlayState, inactive: bool) -> Option<Command> {
        // Played once with sound and done: back to the muted loop.
        if self.sound && state == PlayState::Ended {
            self.sound = false;
            self.paused = false;
            return Some(Command::BackToLoop);
        }
        let wants = self.wants_play(inactive);
        match state {
            // An opening player starts by itself once open, if asked.
            PlayState::Opening | PlayState::Failed => None,
            state if wants && !state.animates() => Some(Command::Play),
            state if !wants && state.animates() => Some(Command::Pause),
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
    render: u64,
    /// The history rendered (and swept the players) this frame.
    swept: bool,
    /// The window is in the background: muted loops hold still.
    inactive: bool,
    /// The window's display scale (device pixels per point); 0 until the
    /// first frame reports it.
    scale: f32,
    /// The timer that releases players off screen for [`RELEASE_AFTER`].
    release_timer: Option<Task<()>>,
}

/// The players (see the module docs). Shared (`Rc`) with the history's
/// animation layer, which draws their pictures.
#[derive(Default)]
pub(super) struct InlineVideos {
    state: RefCell<State>,
}

impl InlineVideos {
    /// A short borrow of the state; `None` when it is already borrowed
    /// further up the stack (never expected: the call is skipped).
    fn with<R>(&self, f: impl FnOnce(&mut State) -> R) -> Option<R> {
        self.state
            .try_borrow_mut()
            .ok()
            .map(|mut state| f(&mut state))
    }

    /// Start a history render pass: players whose rows didn't render in
    /// the last pass stop, and each player's paint count is noted. Once the
    /// pass has painted (deferred to the end of this frame), the players
    /// not painted in view are paused ([`Self::sweep`]).
    pub(super) fn begin_render(self: &Rc<Self>, cx: &mut App) {
        let dropped = self.with(|state| {
            state.swept = true;
            state.render += 1;
            let render = state.render;
            let (keep, drop): (HashMap<Key, Slot>, HashMap<Key, Slot>) =
                std::mem::take(&mut state.players)
                    .into_iter()
                    .partition(|(_, slot)| slot.seen + 1 >= render);
            state.players = keep;
            drop
        });
        let marks: Vec<(Key, VideoPlayer)> = self
            .with(|state| {
                state
                    .players
                    .iter()
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
                if let Some(slot) = state.players.get_mut(&key) {
                    slot.shown_mark = mark;
                }
            }
        });
        drop(dropped);
        let videos = self.clone();
        cx.defer(move |cx| videos.sweep(cx));
    }

    /// The off-screen policy after a history pass painted (or when the
    /// release timer fires): pause what was not painted in view, resume
    /// what was, release what has been off screen for [`RELEASE_AFTER`].
    pub(super) fn sweep(self: &Rc<Self>, cx: &mut App) {
        let now = cx.background_executor().now();
        let players: Vec<(Key, VideoPlayer, u64)> = self
            .with(|state| {
                state
                    .players
                    .iter()
                    .filter(|((chat, _), _)| *chat != PANEL_CHAT)
                    .map(|(key, slot)| (*key, slot.player.clone(), slot.shown_mark))
                    .collect()
            })
            .unwrap_or_default();
        let visible: Vec<(Key, bool)> = players
            .into_iter()
            .map(|(key, player, mark)| (key, player.shown_count() > mark))
            .collect();
        let Some((commands, released, hidden)) = self.with(|state| {
            let inactive = state.inactive;
            let mut commands = Vec::new();
            let mut released = Vec::new();
            for (key, visible) in visible {
                let Some(slot) = state.players.get_mut(&key) else {
                    continue;
                };
                match off_screen_step(visible, slot.hidden_since, now) {
                    OffScreenStep::Show => slot.hidden_since = None,
                    OffScreenStep::Hide => slot.hidden_since = Some(now),
                    OffScreenStep::Stay => {}
                    OffScreenStep::Release => {
                        if let Some(slot) = state.players.remove(&key) {
                            released.push(slot);
                        }
                        continue;
                    }
                }
                if let Some(command) = slot.settle(slot.player.state(), inactive) {
                    commands.push((slot.player.clone(), command));
                }
            }
            let hidden = state
                .players
                .values()
                .any(|slot| slot.hidden_since.is_some());
            (commands, released, hidden)
        }) else {
            return;
        };
        run(commands);
        drop(released);
        if hidden {
            self.arm_release(cx);
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
            cx.update(|cx| videos.sweep(cx));
        });
        self.with(|state| state.release_timer = Some(timer));
    }

    /// Called at the start of every app frame: when the last frame
    /// rendered no history (no chat open, Contacts, Calls…), nothing can
    /// show a clip, so every player stops. `scale` is the window's display
    /// scale, for the size new players decode at.
    pub(super) fn frame_start(&self, history_drawn: bool, scale: f32) {
        let dropped = self.with(|state| {
            let mut dropped = HashMap::new();
            if history_drawn && !state.swept {
                dropped = std::mem::take(&mut state.players);
            }
            // The display or interface scale changed: running players
            // decode at the old size, so restart them at the new one.
            if state.scale > 0. && (state.scale - scale).abs() > f32::EPSILON {
                dropped.extend(std::mem::take(&mut state.players));
            }
            state.swept = false;
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
            let state = player.state();
            if wants && !state.animates() && state != PlayState::Opening {
                player.play();
            } else if !wants && state.animates() {
                player.pause();
            }
        }
    }

    /// Stop everything (viewer opened, autoplay turned off).
    pub(super) fn clear(&self) {
        let dropped = self.with(|state| std::mem::take(&mut state.players));
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
            for (other, slot) in &mut state.players {
                if *other != key && slot.sound {
                    slot.sound = false;
                    slot.paused = false;
                    commands.push((slot.player.clone(), Command::BackToLoop));
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
    /// rendering, so a player paused off screen resumes. A clip that would
    /// take the decoded memory past [`INLINE_DECODE_BUDGET`] doesn't start
    /// (its still shows). `duration` (the message's) shows until the
    /// file's own is known.
    pub(super) fn frame(
        &self,
        key: Key,
        tile: InlineTile,
        duration: Option<f64>,
        path: impl FnOnce() -> Option<PathBuf>,
        cx: &mut App,
    ) -> Option<InlineFrame> {
        let exists = self.with(|state| state.players.contains_key(&key))?;
        if !exists {
            self.open(key, tile, duration, path, cx)?;
        }
        let player = self.with(|state| {
            let render = state.render;
            let slot = state.players.get_mut(&key)?;
            slot.seen = render;
            slot.hidden_since = None;
            Some(slot.player.clone())
        })??;
        self.settled_frame(key, player.state())
    }

    /// The current frame of a running clip, for the history's animation
    /// layer: [`Self::frame`] without starting a player or marking its row
    /// rendered.
    pub(super) fn current(&self, key: Key) -> Option<InlineFrame> {
        let player =
            self.with(|state| state.players.get(&key).map(|slot| slot.player.clone()))??;
        self.settled_frame(key, player.state())
    }

    /// Bring the slot's player in line (loop back, resume, pause) and
    /// return its frame; `None` once its player failed (the still shows).
    fn settled_frame(&self, key: Key, state: PlayState) -> Option<InlineFrame> {
        if state == PlayState::Failed {
            return None;
        }
        let (command, frame) = self.with(|videos| {
            let inactive = videos.inactive;
            let slot = videos.players.get_mut(&key)?;
            let command = slot
                .settle(state, inactive)
                .map(|command| (slot.player.clone(), command));
            let held = slot.paused || slot.seeking.is_some();
            Some((command, slot.frame(held)))
        })??;
        run(command.into_iter().collect());
        Some(frame)
    }

    /// Open the player for `key`, if the budget admits it.
    fn open(
        &self,
        key: Key,
        tile: InlineTile,
        duration: Option<f64>,
        path: impl FnOnce() -> Option<PathBuf>,
        cx: &mut App,
    ) -> Option<()> {
        let (held, scale, inactive, render) = self.with(|state| {
            (
                state.players.values().map(|slot| slot.bytes).sum::<usize>(),
                state.scale,
                state.inactive,
                state.render,
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
        let player =
            VideoPlayer::open(&path()?, Purpose::Inline { max_edge }, duration, cx).ok()?;
        player.set_volume(0.0);
        player.set_looping(true);
        // Behind another app a new muted loop waits for activation.
        if !inactive {
            player.play();
        }
        let mark = player.shown_count();
        let replaced = self.with(|state| {
            state.players.insert(
                key,
                Slot {
                    player,
                    bytes,
                    seen: render,
                    shown_mark: mark,
                    hidden_since: None,
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

    /// The slot's player, for tests.
    #[cfg(test)]
    pub(super) fn player(&self, key: Key) -> Option<VideoPlayer> {
        self.with(|state| state.players.get(&key).map(|slot| slot.player.clone()))
            .flatten()
    }

    /// Whether the slot is paused off screen, for tests.
    #[cfg(test)]
    pub(super) fn is_hidden(&self, key: Key) -> bool {
        self.with(|state| {
            state
                .players
                .get(&key)
                .is_some_and(|slot| slot.hidden_since.is_some())
        })
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
            .checked_sub(RELEASE_AFTER - Duration::from_millis(1))
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
