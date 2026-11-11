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

/// The active track: none, a voice note or a music file with everything
/// that exists only while it is active. One value, so stopping the track
/// drops its slider, scrub, hover and bar state with it.
#[derive(Default)]
pub(super) enum ActiveTrack {
    #[default]
    None,
    Voice(Track),
    Audio(Track),
}

/// What belongs to the active track.
pub(super) struct Track {
    pub(super) message: MessageId,
    /// Chat the track belongs to (may differ from the open chat): the
    /// bar's, and the claim that keeps its history whole.
    pub(super) chat: ChatId,
    /// Length of the loaded track (TDLib's rounded duration).
    pub(super) length: f64,
    pub(super) meta: super::player_bar::TrackMeta,
    /// Interactive seek slider bound to the active row (Phase 4.6).
    pub(super) slider: Entity<SliderState>,
    /// The bar's own progress slider (the row has another one), one for
    /// each window that draws the bar (`None`: the main window). A slider
    /// keeps the geometry of its last paint and maps a press through it,
    /// so one slider drawn in two windows sought with the other window's
    /// layout (docs/decisions/lock-field-per-window.md, "Rejected").
    pub(super) bars: HashMap<Option<ChatId>, Entity<SliderState>>,
    /// The seek slider drag (Change without Release yet).
    pub(super) scrub: Scrub,
    /// Where the pointer rests over a seek control of the track.
    pub(super) hover: Option<SeekHover>,
}

impl Track {
    pub(super) fn new(
        message: MessageId,
        chat: ChatId,
        length: f64,
        meta: super::player_bar::TrackMeta,
        slider: Entity<SliderState>,
    ) -> Self {
        Self {
            message,
            chat,
            length,
            meta,
            slider,
            bars: HashMap::new(),
            scrub: Scrub::Idle,
            hover: None,
        }
    }
}

impl ActiveTrack {
    /// Tests: a voice note or music track of `chat`, with a fresh slider.
    #[cfg(test)]
    pub(super) fn for_test(
        cx: &mut impl AppContext,
        voice: bool,
        chat: ChatId,
        message: MessageId,
        length: f64,
    ) -> Self {
        let slider = cx.new(|_| SliderState::new());
        let meta = super::player_bar::TrackMeta {
            title: String::new().into(),
            subtitle: String::new(),
        };
        let track = Track::new(message, chat, length, meta, slider);
        if voice {
            Self::Voice(track)
        } else {
            Self::Audio(track)
        }
    }

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
            Self::Voice(track) => Some(track.message),
            _ => None,
        }
    }

    pub(super) fn audio(&self) -> Option<MessageId> {
        match self {
            Self::Audio(track) => Some(track.message),
            _ => None,
        }
    }

    pub(super) fn id(&self) -> Option<MessageId> {
        self.track().map(|track| track.message)
    }

    /// The chat of the active track.
    pub(super) fn chat(&self) -> Option<ChatId> {
        self.track().map(|track| track.chat)
    }

    /// Length of the active track; `None` when no row is active.
    pub(super) fn length(&self) -> Option<f64> {
        self.track().map(|track| track.length)
    }

    pub(super) fn scrub(&self) -> Scrub {
        self.track().map_or(Scrub::Idle, |track| track.scrub)
    }

    pub(super) fn hover(&self) -> Option<SeekHover> {
        self.track().and_then(|track| track.hover)
    }

    pub(super) fn slider(&self) -> Option<&Entity<SliderState>> {
        self.track().map(|track| &track.slider)
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
    /// active track, with its length, sliders, drag and hover; they share
    /// the one engine, and whether it plays, and where, comes from `audio`.
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
    /// with the window and chat it plays in, its decode and its pictures.
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

/// A GIF decode running off the UI thread, held by the stage that waits for
/// it. Dropping it (a stop, another playback, a closed window or chat)
/// tells the decode to end before its next frame.
pub(crate) struct GifDecode {
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl GifDecode {
    pub(super) fn start() -> Self {
        Self {
            cancel: std::sync::Arc::default(),
        }
    }

    /// The flag the decoder polls.
    pub(super) fn flag(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.cancel.clone()
    }

    /// `flag` is this decode's: an answer from an earlier decode of the
    /// same GIF is not.
    pub(super) fn is(&self, flag: &std::sync::Arc<std::sync::atomic::AtomicBool>) -> bool {
        std::sync::Arc::ptr_eq(&self.cancel, flag)
    }
}

impl Drop for GifDecode {
    fn drop(&mut self) {
        self.cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// The looping GIF of the history, one value. It leaves for `Idle` on
/// `stop_animation_playback` (a stop, another playback, a closed window or
/// chat), which drops the clip and cancels the decode with it.
#[derive(Default)]
pub(crate) enum GifPlayback {
    #[default]
    Idle,
    /// Chosen; the first picture is still being decoded.
    Waiting {
        playing: PlayingGif,
        decode: GifDecode,
    },
    /// Playing: the held pictures (`gif_clip`), shared with the history row
    /// (and its animation layer) that draws them. The rest of the GIF may
    /// still be decoding.
    Playing {
        playing: PlayingGif,
        clip: std::sync::Arc<super::gif_clip::GifClip>,
        started: Instant,
        decode: GifDecode,
    },
}

impl GifPlayback {
    /// A chosen GIF waiting for the decode `decode`.
    pub(super) fn waiting(playing: PlayingGif, decode: GifDecode) -> Self {
        Self::Waiting { playing, decode }
    }

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

    /// The decode that answers to `flag` is the one this stage holds.
    pub(super) fn is_decode(
        &self,
        flag: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> bool {
        match self {
            Self::Idle => false,
            Self::Waiting { decode, .. } | Self::Playing { decode, .. } => decode.is(flag),
        }
    }

    /// The first picture is in: the chosen GIF plays from `started`.
    pub(super) fn begin_clip(
        &mut self,
        clip: std::sync::Arc<super::gif_clip::GifClip>,
        started: Instant,
    ) {
        if let Self::Waiting { playing, decode } = std::mem::take(self) {
            *self = Self::Playing {
                playing,
                clip,
                started,
                decode,
            };
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
        self.active.length()
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
    use super::{ActiveTrack, PlaybackUi};
    use crate::ui::audio::engine::test_support::{RATE, device, pull, track};
    use crate::ui::audio::{AudioEngine, EngineState, SharedOutput, Simulation};
    use gpui_kit::TestAppContext;
    use quill::ids::{ChatId, MessageId};
    use std::path::Path;

    /// A music track `id` of `length` seconds, as `begin_track_playback`
    /// makes it.
    fn music(cx: &mut TestAppContext, id: i64, length: f64) -> ActiveTrack {
        ActiveTrack::for_test(cx, false, ChatId(1), MessageId(id), length)
    }

    fn ui_without_output() -> (PlaybackUi, AudioEngine) {
        (
            PlaybackUi::new(),
            AudioEngine::new(SharedOutput::for_test(None)),
        )
    }

    fn ui_on(output: &SharedOutput) -> (PlaybackUi, AudioEngine) {
        (PlaybackUi::new(), AudioEngine::new(output.clone()))
    }

    #[gpui_kit::test]
    fn a_failed_start_never_shows_playing(cx: &mut TestAppContext) {
        let (mut ui, mut audio) = ui_without_output();
        ui.active = music(cx, 1, 30.0);
        let started = audio
            .start(Path::new("missing.ogg"), 4.0, 1.0, 1.0, 30.0)
            .is_ok();
        assert!(!started);
        assert_eq!(ui.engine_state(&audio), EngineState::Failed);
        assert!(!ui.is_playing(&audio));
        assert_eq!(ui.active_position_secs(&audio), Some(4.0));
    }

    /// tdesktop shows Pause and the position asked for from the press on.
    #[gpui_kit::test]
    fn a_track_still_building_shows_pause_at_its_offset(cx: &mut TestAppContext) {
        let (output, _device) = device();
        let (mut ui, mut audio) = ui_on(&output);
        ui.active = music(cx, 1, 4.0);
        let path = track("ui-loading.wav", 4);
        audio.start(&path, 2.0, 1.0, 1.0, 4.0).unwrap();
        assert_eq!(ui.engine_state(&audio), EngineState::Loading);
        assert!(ui.is_playing(&audio));
        assert_eq!(ui.active_position_secs(&audio), Some(2.0));
    }

    /// tdesktop's audio row reads no waiting state: a start shows Pause
    /// alone however long the file takes to open.
    #[gpui_kit::test]
    fn a_long_start_shows_pause_alone_and_draws_nothing_new(cx: &mut TestAppContext) {
        let (output, _device) = device();
        let (mut ui, mut audio) = ui_on(&output);
        let clock;
        (audio, clock) = audio.with_manual_clock();
        ui.active = music(cx, 1, 4.0);
        let path = track("ui-long-start.wav", 4);
        audio.start(&path, 2.0, 1.0, 1.0, 4.0).unwrap();
        let pressed = ui.track_shown(&audio, (false, false));
        clock.store(5000, std::sync::atomic::Ordering::Relaxed);
        assert!(ui.is_playing(&audio));
        assert_eq!(ui.track_shown(&audio, (false, false)), pressed);
    }

    #[gpui_kit::test]
    fn a_failed_resume_keeps_the_button_on_play(cx: &mut TestAppContext) {
        let (output, _device) = device();
        let (mut ui, mut audio) = ui_on(&output);
        let path = track("ui-resume.wav", 4);
        ui.active = music(cx, 1, 4.0);
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

    #[gpui_kit::test]
    fn the_bar_follows_consumed_frames_not_the_wall_clock(cx: &mut TestAppContext) {
        let (output, mut source) = device();
        let (mut ui, mut audio) = ui_on(&output);
        let path = track("ui-position.wav", 4);
        ui.active = music(cx, 1, 4.0);
        audio.start(&path, 0.0, 1.0, 1.0, 4.0).unwrap();
        audio.await_landing();
        std::thread::sleep(std::time::Duration::from_millis(250));
        assert!(ui.active_position_secs(&audio).unwrap_or(9.0) < 0.05);
        pull(&mut source, usize::try_from(RATE).unwrap() * 2);
        let secs = ui.active_position_secs(&audio).unwrap_or(0.0);
        assert!((1.8..=2.3).contains(&secs), "got {secs}");
    }

    #[gpui_kit::test]
    fn a_track_that_ended_resets_the_button(cx: &mut TestAppContext) {
        let (mut ui, mut audio) = ui_without_output();
        ui.active = music(cx, 1, 5.0);
        audio.set_simulation(Some(Simulation {
            fail: false,
            stall: false,
        }));
        audio.start(Path::new("a.ogg"), 5.0, 1.0, 1.0, 5.0).unwrap();
        assert_eq!(ui.engine_state(&audio), EngineState::Ended);
        assert!(!ui.is_playing(&audio));
        assert_eq!(ui.active_position_secs(&audio), Some(5.0));
    }

    #[gpui_kit::test]
    fn the_length_belongs_to_the_active_track(cx: &mut TestAppContext) {
        let (mut ui, audio) = ui_without_output();
        assert_eq!(ui.track_duration(), None);
        assert_eq!(ui.active_position_secs(&audio), None);
        ui.active = ActiveTrack::for_test(cx, true, ChatId(1), MessageId(2), 30.0);
        assert_eq!(ui.track_duration(), Some(30.0));
        // The track's length, slider and drag leave with the track.
        ui.active = ActiveTrack::None;
        assert_eq!(ui.track_duration(), None);
        assert!(ui.active.slider().is_none());
    }

    #[gpui_kit::test]
    fn no_track_means_no_position(_cx: &mut TestAppContext) {
        let (ui, audio) = ui_without_output();
        assert_eq!(ui.active_position_secs(&audio), None);
        assert!(!ui.is_playing(&audio));
    }
}
