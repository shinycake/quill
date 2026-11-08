//! What plays for the account on screen: the voice or music track, the
//! GIF, seek state and the player bar. The engines and the shared players
//! are the app's (`players_state`).
#![deny(clippy::indexing_slicing)]

use gpui_kit::component::slider::SliderState;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::ids::FileId;
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
    /// History row whose voice note is playing.
    pub(super) player: super::player_bar::PlayerBarState,
    pub(super) playing_voice: Option<MessageId>,
    /// History row whose music file (`messageAudio`) is playing. Shares `voice_player`.
    pub(super) playing_audio: Option<MessageId>,
    /// What a press asked to play once its file is downloaded (`play_intent`).
    pub(super) intent: Option<super::play_intent::PlayIntent>,
    /// The right-click menu of a Playlist page track.
    pub(super) playlist_menu: Option<super::playlist_manage::PlaylistMenu>,
    /// A track of your own playlist waiting for the "remove" confirmation.
    pub(super) playlist_remove: Option<FileId>,
    /// Length of the loaded track (TDLib's rounded duration). Plain data:
    /// whether a track is active comes from `playing_voice`/`playing_audio`
    /// (see `track_duration`) and whether it plays, and where, from `audio`.
    pub(super) track_length: Option<f64>,
    /// Interactive seek slider bound to the active row (Phase 4.6).
    pub(super) seek_slider: Option<Entity<SliderState>>,
    /// True while the user is dragging the seek slider (Change without Release yet).
    pub(super) seek_scrubbing: bool,
    /// Drag preview position in seconds, shown in the time label while scrubbing.
    pub(super) seek_preview_secs: Option<f64>,
    /// Where the pointer rests over a seek control of the active track.
    pub(super) seek_hover: Option<SeekHover>,
    /// Guard for the playback progress tick task.
    pub(super) tick: bool,
    /// What the seek bars, the player bar and the Touch Bar showed of the
    /// track at the last tick: the tick redraws them only when it changed.
    pub(super) tick_shown: Option<TrackShown>,
    /// Last known position per message, so rows keep their seek bar fill (and
    /// resume from it) after pause/stop.
    pub(super) positions: HashMap<MessageId, f64>,
    /// History row whose `image/gif` file is looping (`inline_playback`).
    pub(super) playing_animation: Option<MessageId>,
    pub(super) autoplayed_gifs: std::collections::HashSet<MessageId>,
    /// The playing GIF's held pictures (`gif_clip`), once decoded. Shared
    /// with the history row (and its animation layer) that draws it.
    pub(super) gif: Option<std::sync::Arc<super::gif_clip::GifClip>>,
    pub(super) gif_started_at: Option<Instant>,
    /// The GIF decodes running, by message (one per GIF at a time).
    pub(super) gif_decodes: HashMap<MessageId, GifDecode>,
    /// A linked `?t=` media timestamp waiting for its message to load
    /// (chat, message, seconds, when to give up).
    pub(super) pending_media_seek: Option<(ChatId, quill::ids::MessageId, i32, std::time::Instant)>,
    /// The media viewer's playback error. The audio rows read
    /// `audio.error()` instead.
    pub(super) error: Option<String>,
}

impl PlaybackUi {
    /// The playing GIF, for its history row.
    pub(super) fn gif_play(&self) -> Option<super::gif_clip::GifPlay> {
        let clip = self.gif.clone()?;
        Some(super::gif_clip::GifPlay::new(
            clip,
            self.gif_started_at.unwrap_or_else(Instant::now),
        ))
    }

    /// Length of the active track; `None` when no row is active.
    pub(super) fn track_duration(&self) -> Option<f64> {
        self.playing_voice.or(self.playing_audio)?;
        self.track_length
    }

    pub(super) fn new() -> Self {
        Self {
            player: Default::default(),
            playing_voice: None,
            playing_audio: None,
            intent: None,
            playlist_menu: None,
            playlist_remove: None,
            track_length: None,
            seek_slider: None,
            seek_scrubbing: false,
            seek_preview_secs: None,
            seek_hover: None,
            tick: false,
            tick_shown: None,
            positions: HashMap::new(),
            playing_animation: None,
            autoplayed_gifs: Default::default(),
            gif: None,
            gif_started_at: None,
            gif_decodes: HashMap::new(),
            pending_media_seek: None,
            error: None,
        }
    }
}

/// A GIF decode running for a message.
pub(crate) struct GifDecode {
    /// Set when playback stops; the decode then ends before its next frame.
    pub(super) cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// The clip, once its first picture is in: a replay while the rest
    /// still decodes plays it at once.
    pub(super) first: Option<std::sync::Arc<super::gif_clip::GifClip>>,
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
    use super::PlaybackUi;
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
        ui.playing_audio = Some(MessageId(1));
        ui.track_length = Some(30.0);
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
        ui.playing_audio = Some(MessageId(1));
        ui.track_length = Some(4.0);
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
        ui.playing_audio = Some(MessageId(1));
        ui.track_length = Some(4.0);
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
        ui.playing_audio = Some(MessageId(1));
        ui.track_length = Some(4.0);
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
        let (mut ui, mut audio) = ui_on(&output);
        let path = track("ui-position.wav", 4);
        ui.playing_audio = Some(MessageId(1));
        ui.track_length = Some(4.0);
        audio.start(&path, 0.0, 1.0, 1.0, 4.0).unwrap();
        audio.await_landing();
        std::thread::sleep(std::time::Duration::from_millis(250));
        assert!(ui.active_position_secs(&audio).unwrap_or(9.0) < 0.05);
        pull(&mut source, usize::try_from(RATE).unwrap() * 2);
        let secs = ui.active_position_secs(&audio).unwrap_or(0.0);
        assert!((1.8..=2.3).contains(&secs), "got {secs}");
    }

    #[test]
    fn a_track_that_ended_resets_the_button() {
        let (mut ui, mut audio) = ui_without_output();
        ui.playing_audio = Some(MessageId(1));
        ui.track_length = Some(5.0);
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
    fn a_stale_length_never_makes_a_track_active() {
        let (mut ui, audio) = ui_without_output();
        ui.track_length = Some(30.0);
        assert_eq!(ui.track_duration(), None);
        assert_eq!(ui.active_position_secs(&audio), None);
        ui.playing_voice = Some(MessageId(2));
        assert_eq!(ui.track_duration(), Some(30.0));
    }

    #[test]
    fn no_track_means_no_position() {
        let (ui, audio) = ui_without_output();
        assert_eq!(ui.active_position_secs(&audio), None);
        assert!(!ui.is_playing(&audio));
    }
}
