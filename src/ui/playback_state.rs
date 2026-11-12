//! What plays for the account on screen: the voice or music track, the
//! GIF, seek state and the player bar. The engines and the shared players
//! are the app's (`players_state`).
#![deny(clippy::indexing_slicing)]

use gpui_kit::component::slider::SliderState;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::ids::MessageId;
use std::collections::HashMap;
use std::time::Instant;

/// Which seek control of the active track the pointer is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SeekSite {
    /// The active voice or music row in the history.
    Row,
    PlayerBar,
}

/// The pointer over a seek control, as a fraction of its width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SeekHover {
    pub(crate) site: SeekSite,
    pub(crate) at: f32,
}

pub(crate) use quill::ids::ChatMessage;

/// The active track: none, a voice note or a music file, with everything
/// that is about it. Ending the track drops all of it together.
#[derive(Default)]
pub(super) enum ActiveTrack {
    #[default]
    None,
    Voice(Track),
    Audio(Track),
}

/// What is about the track on the engine.
pub(super) struct Track {
    pub(super) id: MessageId,
    /// The chat the track belongs to (may differ from the open chat).
    pub(super) chat: Option<ChatId>,
    /// What the player bar shows about it.
    pub(super) meta: Option<super::player_bar::TrackMeta>,
    /// Length of the loaded track (TDLib's rounded duration).
    pub(super) length: Option<f64>,
    /// Interactive seek slider bound to the track's row (Phase 4.6).
    pub(super) slider: Option<Entity<SliderState>>,
    /// The seek slider drag (Change without Release yet).
    pub(super) scrub: Scrub,
    /// Where the pointer rests over a seek control of the track.
    pub(super) hover: Option<SeekHover>,
}

impl Track {
    #[cfg(test)]
    pub(super) fn new(id: MessageId) -> Self {
        Self {
            id,
            chat: None,
            meta: None,
            length: None,
            slider: None,
            scrub: Scrub::Idle,
            hover: None,
        }
    }

    #[cfg(test)]
    pub(super) fn with_length(mut self, length: f64) -> Self {
        self.length = Some(length);
        self
    }

    #[cfg(test)]
    pub(super) fn in_chat(mut self, chat: ChatId) -> Self {
        self.chat = Some(chat);
        self
    }
}

impl ActiveTrack {
    pub(super) fn track(&self) -> Option<&Track> {
        match self {
            Self::None => None,
            Self::Voice(track) | Self::Audio(track) => Some(track),
        }
    }

    pub(super) fn track_mut(&mut self) -> Option<&mut Track> {
        match self {
            Self::None => None,
            Self::Voice(track) | Self::Audio(track) => Some(track),
        }
    }

    pub(super) fn voice(&self) -> Option<MessageId> {
        match self {
            Self::Voice(track) => Some(track.id),
            _ => None,
        }
    }

    pub(super) fn audio(&self) -> Option<MessageId> {
        match self {
            Self::Audio(track) => Some(track.id),
            _ => None,
        }
    }

    pub(super) fn id(&self) -> Option<MessageId> {
        self.track().map(|track| track.id)
    }

    /// The chat the track belongs to.
    pub(super) fn chat(&self) -> Option<ChatId> {
        self.track()?.chat
    }

    /// The voice note ends; its id.
    pub(super) fn take_voice(&mut self) -> Option<MessageId> {
        let id = self.voice()?;
        *self = Self::None;
        Some(id)
    }

    /// The music file ends; its id.
    pub(super) fn take_audio(&mut self) -> Option<MessageId> {
        let id = self.audio()?;
        *self = Self::None;
        Some(id)
    }
}

/// The user's drag on the seek slider: the position it shows in the time
/// label meanwhile, in seconds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) enum Scrub {
    #[default]
    Idle,
    Dragging(f64),
}

impl Scrub {
    pub(super) fn is_dragging(self) -> bool {
        matches!(self, Self::Dragging(_))
    }

    pub(super) fn preview(self) -> Option<f64> {
        match self {
            Self::Dragging(secs) => Some(secs),
            Self::Idle => None,
        }
    }
}

/// The playback progress tick: no task, or one running with what the seek
/// bars, the player bar and the Touch Bar showed of the track at its last
/// pass (it redraws them only when that changed). Dropping the stage drops
/// the task.
pub(super) enum PlaybackTick {
    Idle,
    Running {
        shown: Option<TrackShown>,
        _task: gpui_kit::Task<()>,
    },
}

impl PlaybackTick {
    pub(super) fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }

    /// Record what the pass showed; whether it differs from the pass
    /// before.
    pub(super) fn note_shown(&mut self, now: TrackShown) -> bool {
        match self {
            Self::Running { shown, .. } => shown.replace(now) != Some(now),
            Self::Idle => false,
        }
    }
}

/// The active track as the seek bars, the player bar and the Touch Bar
/// draw it (`PlaybackUi::track_shown`). A paused track's stays the same
/// until its chat gains or loses a song.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TrackShown {
    state: super::audio::EngineState,
    position_bits: Option<u64>,
    error: Option<super::audio::AudioError>,
    /// Previous / next (`QuillApp::playlist_can_move`): the songs loaded
    /// in the track's chat, which a message arriving there off screen
    /// changes without redrawing the bar.
    can_move: (bool, bool),
}

pub(crate) struct PlaybackUi {
    pub(super) player: super::player_bar::PlayerBarState,
    /// History row whose voice note or music file (`messageAudio`) is the
    /// active track; they share the one engine.
    pub(super) active: ActiveTrack,
    /// What a press asked to play once its file is downloaded (`play_intent`).
    pub(super) intent: Option<super::play_intent::PlayIntent>,
    /// The right-click menu of a Playlist page track, or the "remove"
    /// confirmation of one of your own tracks.
    pub(super) playlist: super::playlist_manage::PlaylistFlow,
    /// The playback progress tick task, while one runs.
    pub(super) tick: PlaybackTick,
    /// Last known position per chat message, so rows keep their seek bar fill (and
    /// resume from it) after pause/stop.
    pub(super) positions: HashMap<ChatMessage, f64>,
    /// History row whose `image/gif` file is looping (`inline_playback`),
    /// with the window and chat it plays in, and its pictures once decoded.
    pub(super) animation: GifPlayback,
    /// GIFs that autoplayed once, by chat: leaving a chat resets only
    /// its own, so another window's GIF does not start over.
    pub(super) autoplayed_gifs: std::collections::HashSet<ChatMessage>,
    /// A linked `?t=` media timestamp waiting for its message to load
    /// (chat, message, seconds, when to give up).
    pub(super) pending_media_seek: Option<(ChatId, quill::ids::MessageId, i32, std::time::Instant)>,
}

/// The `image/gif` file looping in a history row: the row of one window,
/// in one chat. Message ids repeat across chats, and a window that closes
/// takes its row with it (`forget_chat_window` stops it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PlayingGif {
    /// The window whose row plays it (`None`: the main window).
    pub(super) window: Option<ChatId>,
    pub(super) message: ChatMessage,
}

/// The decode of a GIF running off the UI thread. The stage that holds it
/// owns it: dropping the stage drops the task (its landing never runs) and
/// tells the decoder to end before its next frame.
pub(crate) struct GifRun {
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    _task: gpui_kit::Task<()>,
}

impl GifRun {
    pub(super) fn new(
        cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
        task: gpui_kit::Task<()>,
    ) -> Self {
        Self {
            cancel,
            _task: task,
        }
    }

    /// Whether the decoder was told to stop.
    #[cfg(test)]
    pub(super) fn cancelled(&self) -> bool {
        self.cancel.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl Drop for GifRun {
    fn drop(&mut self) {
        self.cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// The looping GIF of the history, one value. It leaves for `Idle` on
/// `stop_animation_playback` (a stop, another playback, a closed window or
/// chat, the screen going away), which drops the clip and the decode with
/// it.
#[derive(Default)]
pub(crate) enum GifPlayback {
    #[default]
    Idle,
    /// Chosen; the first picture is still being decoded.
    Waiting { playing: PlayingGif, run: GifRun },
    /// Playing: the held pictures (`gif_clip`), shared with the history row
    /// (and its animation layer) that draws them. `run` is the decode still
    /// producing the rest of them.
    Playing {
        playing: PlayingGif,
        clip: std::sync::Arc<super::gif_clip::GifClip>,
        started: Instant,
        run: Option<GifRun>,
    },
}

impl GifPlayback {
    pub(super) fn playing(&self) -> Option<PlayingGif> {
        match self {
            Self::Idle => None,
            Self::Waiting { playing, .. } | Self::Playing { playing, .. } => Some(*playing),
        }
    }

    pub(super) fn clip(&self) -> Option<&std::sync::Arc<super::gif_clip::GifClip>> {
        match self {
            Self::Playing { clip, .. } => Some(clip),
            _ => None,
        }
    }

    /// The first picture is in: the chosen GIF plays from `started`, its
    /// decode still running.
    pub(super) fn begin_clip(
        &mut self,
        clip: std::sync::Arc<super::gif_clip::GifClip>,
        started: Instant,
    ) {
        *self = match std::mem::take(self) {
            Self::Waiting { playing, run } => Self::Playing {
                playing,
                clip,
                started,
                run: Some(run),
            },
            other => other,
        };
    }

    /// The decode ended: the pictures it made are all there is.
    pub(super) fn decode_ended(&mut self) {
        if let Self::Playing { run, .. } = self {
            *run = None;
        }
    }

    /// A chosen GIF whose decode never ends, for tests.
    #[cfg(test)]
    pub(crate) fn waiting_in_tests(playing: PlayingGif) -> Self {
        Self::Waiting {
            playing,
            run: GifRun::new(
                std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                gpui_kit::Task::ready(()),
            ),
        }
    }

    /// The decode of the chosen GIF is in `run`.
    #[cfg(test)]
    pub(super) fn run(&self) -> Option<&GifRun> {
        match self {
            Self::Waiting { run, .. } => Some(run),
            Self::Playing { run, .. } => run.as_ref(),
            Self::Idle => None,
        }
    }
}

impl PlaybackUi {
    /// The playing GIF, for its history row.
    pub(super) fn gif_play(&self) -> Option<super::gif_clip::GifPlay> {
        match &self.animation {
            GifPlayback::Playing { clip, started, .. } => {
                Some(super::gif_clip::GifPlay::new(clip.clone(), *started))
            }
            _ => None,
        }
    }

    /// Length of the active track; `None` when no row is active.
    pub(super) fn track_duration(&self) -> Option<f64> {
        self.active.track()?.length
    }

    /// The drag on the active track's seek slider.
    pub(super) fn scrub(&self) -> Scrub {
        self.active.track().map_or(Scrub::Idle, |track| track.scrub)
    }

    /// Where the pointer rests over a seek control of the active track.
    pub(super) fn seek_hover(&self) -> Option<SeekHover> {
        self.active.track()?.hover
    }

    /// The active track's seek slider.
    pub(super) fn seek_slider(&self) -> Option<&Entity<SliderState>> {
        self.active.track()?.slider.as_ref()
    }

    pub(super) fn new() -> Self {
        Self {
            player: Default::default(),
            active: ActiveTrack::None,
            intent: None,
            playlist: Default::default(),
            tick: PlaybackTick::Idle,
            positions: HashMap::new(),
            animation: GifPlayback::Idle,
            autoplayed_gifs: Default::default(),
            pending_media_seek: None,
        }
    }
}

impl PlaybackUi {
    /// What the active track's sound is doing (`Idle` with no track).
    pub(super) fn engine_state(
        &self,
        audio: &super::audio::AudioEngine,
    ) -> super::audio::EngineState {
        audio.state()
    }

    /// Paused by the user, with no seek waiting to be applied: nothing
    /// changes until a resume, a seek or a new track.
    pub(super) fn held_paused(&self, audio: &super::audio::AudioEngine) -> bool {
        self.engine_state(audio) == super::audio::EngineState::Paused && !audio.seek_pending()
    }

    /// The play button: true while the sound is being consumed, or on its
    /// way (tdesktop shows Pause for a track that is starting).
    pub(super) fn is_playing(&self, audio: &super::audio::AudioEngine) -> bool {
        use super::audio::EngineState::{Loading, Playing};
        matches!(self.engine_state(audio), Playing | Loading)
    }

    /// What the seek bars, the player bar and the Touch Bar draw of the
    /// active track, with the playlist's `can_move`.
    pub(super) fn track_shown(
        &self,
        audio: &super::audio::AudioEngine,
        can_move: (bool, bool),
    ) -> TrackShown {
        TrackShown {
            state: self.engine_state(audio),
            position_bits: self.active_position_secs(audio).map(f64::to_bits),
            error: audio.error(),
            can_move,
        }
    }

    /// Where the active track is, in seconds, from the frames the output
    /// consumed. `None` with no active track.
    pub(super) fn active_position_secs(&self, audio: &super::audio::AudioEngine) -> Option<f64> {
        let duration = self.track_duration()?;
        Some(audio.position_secs().clamp(0.0, duration.max(0.0)))
    }
}

/// The screen's track read against the app's engine (`players.audio`).
impl super::app::QuillApp {
    pub(super) fn held_paused(&self) -> bool {
        self.playback.held_paused(&self.players.audio)
    }

    pub(super) fn is_playing(&self) -> bool {
        self.playback.is_playing(&self.players.audio)
    }

    pub(super) fn track_shown(&self, can_move: (bool, bool)) -> TrackShown {
        self.playback.track_shown(&self.players.audio, can_move)
    }

    pub(super) fn active_position_secs(&self) -> Option<f64> {
        self.playback.active_position_secs(&self.players.audio)
    }
}

/// The track, the GIF and the seek bar stop with the account: the engine
/// is the app's, so it is told to stop, and the stickers of the old rows
/// stop playing.
impl super::on_screen::Farewell for PlaybackUi {
    fn farewell(
        app: &mut super::app::QuillApp,
        _: &mut Window,
        cx: &mut Context<super::app::QuillApp>,
    ) {
        app.close_player(cx);
        app.stop_sticker_playback();
    }
}

#[cfg(test)]
mod tests {
    use super::{ActiveTrack, PlaybackUi, Track};
    use crate::ui::audio::engine::test_support::{RATE, device, pull, track};
    use crate::ui::audio::{AudioEngine, EngineState, SharedOutput, Simulation};
    use quill::ids::MessageId;
    use std::path::Path;

    fn ui_without_output() -> (PlaybackUi, AudioEngine) {
        (
            PlaybackUi::new(),
            AudioEngine::new(SharedOutput::for_test(None)),
        )
    }

    fn ui_on(output: &SharedOutput) -> (PlaybackUi, AudioEngine) {
        (PlaybackUi::new(), AudioEngine::new(output.clone()))
    }

    #[test]
    fn a_failed_start_never_shows_playing() {
        let (mut ui, mut audio) = ui_without_output();
        ui.active = ActiveTrack::Audio(Track::new(MessageId(1)).with_length(30.0));
        let started = audio
            .start(Path::new("missing.ogg"), 4.0, 1.0, 1.0, 30.0)
            .is_ok();
        assert!(!started);
        assert_eq!(ui.engine_state(&audio), EngineState::Failed);
        assert!(!ui.is_playing(&audio));
        assert_eq!(ui.active_position_secs(&audio), Some(4.0));
    }

    /// tdesktop shows Pause and the position asked for from the press on.
    #[test]
    fn a_track_still_building_shows_pause_at_its_offset() {
        let (output, _device) = device();
        let (mut ui, mut audio) = ui_on(&output);
        ui.active = ActiveTrack::Audio(Track::new(MessageId(1)).with_length(4.0));
        let path = track("ui-loading.wav", 4);
        audio.start(&path, 2.0, 1.0, 1.0, 4.0).unwrap();
        assert_eq!(ui.engine_state(&audio), EngineState::Loading);
        assert!(ui.is_playing(&audio));
        assert_eq!(ui.active_position_secs(&audio), Some(2.0));
    }

    /// tdesktop's audio row reads no waiting state: a start shows Pause
    /// alone however long the file takes to open.
    #[test]
    fn a_long_start_shows_pause_alone_and_draws_nothing_new() {
        let (output, _device) = device();
        let (mut ui, mut audio) = ui_on(&output);
        let clock;
        (audio, clock) = audio.with_manual_clock();
        ui.active = ActiveTrack::Audio(Track::new(MessageId(1)).with_length(4.0));
        let path = track("ui-long-start.wav", 4);
        audio.start(&path, 2.0, 1.0, 1.0, 4.0).unwrap();
        let pressed = ui.track_shown(&audio, (false, false));
        clock.store(5000, std::sync::atomic::Ordering::Relaxed);
        assert!(ui.is_playing(&audio));
        assert_eq!(ui.track_shown(&audio, (false, false)), pressed);
    }

    #[test]
    fn a_failed_resume_keeps_the_button_on_play() {
        let (output, _device) = device();
        let (mut ui, mut audio) = ui_on(&output);
        let path = track("ui-resume.wav", 4);
        ui.active = ActiveTrack::Audio(Track::new(MessageId(1)).with_length(4.0));
        audio.start(&path, 0.0, 1.0, 1.0, 4.0).unwrap();
        audio.await_landing();
        assert!(ui.is_playing(&audio));
        audio.pause();
        assert!(!ui.is_playing(&audio));
        audio.seek(2.0, false).unwrap();
        std::fs::remove_file(&path).unwrap();
        audio.resume(2.0).unwrap();
        audio.await_landing();
        assert!(!ui.is_playing(&audio));
        assert_eq!(ui.engine_state(&audio), EngineState::Paused);
    }

    #[test]
    fn the_bar_follows_consumed_frames_not_the_wall_clock() {
        let (output, mut source) = device();
        let (mut ui, audio) = ui_on(&output);
        let (mut audio, clock) = audio.with_manual_clock();
        let path = track("ui-position.wav", 4);
        ui.active = ActiveTrack::Audio(Track::new(MessageId(1)).with_length(4.0));
        audio.start(&path, 0.0, 1.0, 1.0, 4.0).unwrap();
        audio.await_landing();
        clock.store(250, std::sync::atomic::Ordering::Relaxed);
        assert!(ui.active_position_secs(&audio).unwrap_or(9.0) < 0.05);
        pull(&mut source, usize::try_from(RATE).unwrap() * 2);
        let secs = ui.active_position_secs(&audio).unwrap_or(0.0);
        assert!((1.8..=2.3).contains(&secs), "got {secs}");
    }

    #[test]
    fn a_track_that_ended_resets_the_button() {
        let (mut ui, mut audio) = ui_without_output();
        ui.active = ActiveTrack::Audio(Track::new(MessageId(1)).with_length(5.0));
        audio.set_simulation(Some(Simulation {
            fail: false,
            stall: false,
        }));
        audio.start(Path::new("a.ogg"), 5.0, 1.0, 1.0, 5.0).unwrap();
        assert_eq!(ui.engine_state(&audio), EngineState::Ended);
        assert!(!ui.is_playing(&audio));
        assert_eq!(ui.active_position_secs(&audio), Some(5.0));
    }

    #[test]
    fn the_length_goes_with_the_track() {
        let (mut ui, audio) = ui_without_output();
        assert_eq!(ui.track_duration(), None);
        assert_eq!(ui.active_position_secs(&audio), None);
        ui.active = ActiveTrack::Voice(Track::new(MessageId(2)).with_length(30.0));
        assert_eq!(ui.track_duration(), Some(30.0));
        ui.active.take_voice();
        assert_eq!(ui.track_duration(), None);
    }

    #[test]
    fn no_track_means_no_position() {
        let (ui, audio) = ui_without_output();
        assert_eq!(ui.active_position_secs(&audio), None);
        assert!(!ui.is_playing(&audio));
    }
}
