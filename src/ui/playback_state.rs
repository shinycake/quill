//! Voice, music, GIF, sticker and inline video playback.
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
    loading: bool,
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
    /// In-process player for the active voice note / audio file.
    pub(super) audio: super::audio::AudioEngine,
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
    /// `QuillApp::watch_audio_builds` is running.
    pub(super) builds_watched: bool,
    /// Last known position per message, so rows keep their seek bar fill (and
    /// resume from it) after pause/stop.
    pub(super) positions: HashMap<MessageId, f64>,
    /// Animated stickers, dice and custom emoji everywhere (one player,
    /// shared by every view: `sticker_player`).
    pub(super) stickers: std::rc::Rc<super::sticker_player::Player>,
    /// The lone animated emoji played in the open chat and the replays
    /// taps asked for (`animated_emoji`); cleared with the sticker heads.
    pub(super) emoji_plays: std::cell::RefCell<quill::animated_emoji::EmojiPlays>,
    /// History row whose `image/gif` file is looping (`inline_playback`).
    pub(super) playing_animation: Option<MessageId>,
    pub(super) autoplayed_gifs: std::collections::HashSet<MessageId>,
    /// The playing GIF's held pictures (`gif_clip`), once decoded. Shared
    /// with the history row (and its animation layer) that draws it.
    pub(super) gif: Option<std::rc::Rc<super::gif_clip::GifClip>>,
    pub(super) gif_started_at: Option<Instant>,
    /// The GIF decodes running, by message, and their cancel flags (set
    /// when playback stops; one decode per GIF at a time).
    pub(super) gif_decodes: HashMap<MessageId, std::sync::Arc<std::sync::atomic::AtomicBool>>,
    /// A linked `?t=` media timestamp waiting for its message to load
    /// (chat, message, seconds, when to give up).
    pub(super) pending_media_seek: Option<(ChatId, quill::ids::MessageId, i32, std::time::Instant)>,
    /// Muted, looping inline players for visible videos and GIFs.
    /// Shared with the history's animation layer, which draws the clips'
    /// current frames (`inline_video::LiveSource`).
    pub(super) inline_videos: std::rc::Rc<super::inline_players::InlineVideos>,
    /// Premium users' video userpics beside messages and in the chat
    /// list, shared with the layers that draw them.
    pub(super) userpic_videos: std::rc::Rc<super::userpic_videos::UserpicVideos>,
    /// MED1: playback speed multiplier, 0.5–2.0 (TGX `PlaybackSpeed*`;
    /// applied by the pitch-preserving tempo stretcher + the playback clock rate).
    pub(super) speed: f64,
    /// MED1: playback volume 0.0–1.0 (player volume); 0 is muted.
    pub(super) volume: f32,
    /// MED1: last non-zero volume, restored by the mute toggle.
    pub(super) unmuted_volume: f32,
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

    /// What the active track's sound is doing (`Idle` with no track).
    pub(super) fn engine_state(&self) -> super::audio::EngineState {
        self.audio.state()
    }

    /// The play button: true while the sound is being consumed, or on its
    /// way (tdesktop shows Pause for a track that is starting).
    pub(super) fn is_playing(&self) -> bool {
        use super::audio::EngineState::{Loading, Playing};
        matches!(self.engine_state(), Playing | Loading)
    }

    /// What the seek bars, the player bar and the Touch Bar draw of the
    /// active track, with the playlist's `can_move`.
    pub(super) fn track_shown(&self, can_move: (bool, bool)) -> TrackShown {
        TrackShown {
            state: self.engine_state(),
            position_bits: self.active_position_secs().map(f64::to_bits),
            error: self.audio.error(),
            loading: self.audio.loading_shown(),
            can_move,
        }
    }

    /// Where the active track is, in seconds, from the frames the output
    /// consumed. `None` with no active track.
    pub(super) fn active_position_secs(&self) -> Option<f64> {
        let duration = self.track_duration()?;
        Some(self.audio.position_secs().clamp(0.0, duration.max(0.0)))
    }

    pub(super) fn new(audio_output: &super::audio::SharedOutput) -> Self {
        Self {
            player: Default::default(),
            playing_voice: None,
            playing_audio: None,
            intent: None,
            playlist_menu: None,
            playlist_remove: None,
            audio: super::audio::AudioEngine::new(audio_output.clone()),
            track_length: None,
            seek_slider: None,
            seek_scrubbing: false,
            seek_preview_secs: None,
            seek_hover: None,
            tick: false,
            tick_shown: None,
            builds_watched: false,
            positions: HashMap::new(),
            stickers: std::rc::Rc::new(super::sticker_player::Player::new()),
            emoji_plays: std::cell::RefCell::default(),
            playing_animation: None,
            autoplayed_gifs: Default::default(),
            gif: None,
            gif_started_at: None,
            gif_decodes: HashMap::new(),
            pending_media_seek: None,
            inline_videos: Default::default(),
            userpic_videos: Default::default(),
            speed: 1.0,
            volume: 1.0,
            unmuted_volume: 1.0,
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PlaybackUi;
    use crate::ui::audio::engine::test_support::{RATE, device, pull, track};
    use crate::ui::audio::{EngineState, SharedOutput, Simulation};
    use quill::ids::MessageId;
    use std::path::Path;

    fn ui_without_output() -> PlaybackUi {
        PlaybackUi::new(&SharedOutput::for_test(None))
    }

    #[test]
    fn a_failed_start_never_shows_playing() {
        let mut ui = ui_without_output();
        ui.playing_audio = Some(MessageId(1));
        ui.track_length = Some(30.0);
        let started = ui
            .audio
            .start(Path::new("missing.ogg"), 4.0, 1.0, 1.0, 30.0)
            .is_ok();
        assert!(!started);
        assert_eq!(ui.engine_state(), EngineState::Failed);
        assert!(!ui.is_playing());
        assert_eq!(ui.active_position_secs(), Some(4.0));
    }

    /// tdesktop shows Pause and the position asked for from the press on.
    #[test]
    fn a_track_still_building_shows_pause_at_its_offset() {
        let (output, _device) = device();
        let mut ui = PlaybackUi::new(&output);
        ui.playing_audio = Some(MessageId(1));
        ui.track_length = Some(4.0);
        let path = track("ui-loading.wav", 4);
        ui.audio.start(&path, 2.0, 1.0, 1.0, 4.0).unwrap();
        assert_eq!(ui.engine_state(), EngineState::Loading);
        assert!(ui.is_playing());
        assert_eq!(ui.active_position_secs(), Some(2.0));
    }

    #[test]
    fn a_failed_resume_keeps_the_button_on_play() {
        let (output, _device) = device();
        let mut ui = PlaybackUi::new(&output);
        let path = track("ui-resume.wav", 4);
        ui.playing_audio = Some(MessageId(1));
        ui.track_length = Some(4.0);
        ui.audio.start(&path, 0.0, 1.0, 1.0, 4.0).unwrap();
        ui.audio.await_build();
        assert!(ui.is_playing());
        ui.audio.pause();
        assert!(!ui.is_playing());
        ui.audio.seek(2.0, false).unwrap();
        std::fs::remove_file(&path).unwrap();
        ui.audio.resume(2.0).unwrap();
        ui.audio.await_build();
        assert!(!ui.is_playing());
        assert_eq!(ui.engine_state(), EngineState::Paused);
    }

    #[test]
    fn the_bar_follows_consumed_frames_not_the_wall_clock() {
        let (output, mut source) = device();
        let mut ui = PlaybackUi::new(&output);
        let path = track("ui-position.wav", 4);
        ui.playing_audio = Some(MessageId(1));
        ui.track_length = Some(4.0);
        ui.audio.start(&path, 0.0, 1.0, 1.0, 4.0).unwrap();
        ui.audio.await_build();
        std::thread::sleep(std::time::Duration::from_millis(250));
        assert!(ui.active_position_secs().unwrap_or(9.0) < 0.05);
        pull(&mut source, usize::try_from(RATE).unwrap() * 2);
        let secs = ui.active_position_secs().unwrap_or(0.0);
        assert!((1.8..=2.3).contains(&secs), "got {secs}");
    }

    #[test]
    fn a_track_that_ended_resets_the_button() {
        let mut ui = ui_without_output();
        ui.playing_audio = Some(MessageId(1));
        ui.track_length = Some(5.0);
        ui.audio.set_simulation(Some(Simulation {
            fail: false,
            stall: false,
            slow: false,
        }));
        ui.audio
            .start(Path::new("a.ogg"), 5.0, 1.0, 1.0, 5.0)
            .unwrap();
        assert_eq!(ui.engine_state(), EngineState::Ended);
        assert!(!ui.is_playing());
        assert_eq!(ui.active_position_secs(), Some(5.0));
    }

    #[test]
    fn a_stale_length_never_makes_a_track_active() {
        let mut ui = ui_without_output();
        ui.track_length = Some(30.0);
        assert_eq!(ui.track_duration(), None);
        assert_eq!(ui.active_position_secs(), None);
        ui.playing_voice = Some(MessageId(2));
        assert_eq!(ui.track_duration(), Some(30.0));
    }

    #[test]
    fn no_track_means_no_position() {
        let ui = ui_without_output();
        assert_eq!(ui.active_position_secs(), None);
        assert!(!ui.is_playing());
    }
}
