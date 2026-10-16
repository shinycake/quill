//! Voice, music, GIF, sticker and inline video playback.
#![deny(clippy::indexing_slicing)]

use gpui_kit::component::slider::SliderState;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::ids::FileId;
use quill::ids::MessageId;
use quill::playback::PlaybackClock;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

pub(crate) struct PlaybackUi {
    /// History row whose voice note is playing.
    pub(super) player: super::player_bar::PlayerBarState,
    pub(super) playing_voice: Option<MessageId>,
    /// History row whose music file (`messageAudio`) is playing. Shares `voice_player`.
    pub(super) playing_audio: Option<MessageId>,
    /// Play was tapped before the track was local. Resume when `downloadFile` finishes.
    pub(super) pending_audio_play: Option<(ChatId, MessageId, FileId, f64)>,
    /// The right-click menu of a Playlist page track.
    pub(super) playlist_menu: Option<super::playlist_manage::PlaylistMenu>,
    /// A track of your own playlist waiting for the "remove" confirmation.
    pub(super) playlist_remove: Option<FileId>,
    pub(super) pending_voice_play: Option<(ChatId, MessageId, FileId, bool, f64)>,
    /// In-process player for the active voice note / audio file.
    pub(super) audio: super::audio::AudioEngine,
    /// Active audio/voice track's playback clock (playing or paused-with-offset).
    /// `Some` exactly when `playing_voice` or `playing_audio` is `Some` (Phase 4.6).
    pub(super) clock: Option<PlaybackClock>,
    /// Sandbox-checked local path of the active track, for restarting the sound on seek.
    pub(super) path: Option<PathBuf>,
    /// Interactive seek slider bound to the active row (Phase 4.6).
    pub(super) seek_slider: Option<Entity<SliderState>>,
    /// True while the user is dragging the seek slider (Change without Release yet).
    pub(super) seek_scrubbing: bool,
    /// Drag preview position in seconds, shown in the time label while scrubbing.
    pub(super) seek_preview_secs: Option<f64>,
    /// Guard for the playback progress tick task.
    pub(super) tick: bool,
    /// Last known position per message, so rows keep their seek bar fill (and
    /// resume from it) after pause/stop.
    pub(super) positions: HashMap<MessageId, f64>,
    /// Animated stickers in the history and the panels.
    pub(super) sticker_playback: super::sticker_playback::StickerPlayback,
    /// Animated custom emoji (smaller frames, more clips).
    pub(super) emoji_playback: super::sticker_playback::StickerPlayback,
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
    /// Play was tapped before the clip was local. Resume when `downloadFile` finishes.
    pub(super) pending_gif_play: Option<(MessageId, FileId, String)>,
    /// A linked `?t=` media timestamp waiting for its message to load
    /// (chat, message, seconds, polls waited).
    pub(super) pending_media_seek: Option<(ChatId, quill::ids::MessageId, i32, u32)>,
    /// Muted, looping inline players for visible videos and GIFs.
    /// Shared with the history's animation layer, which draws the clips'
    /// current frames (`inline_video::LiveSource`).
    pub(super) inline_videos: std::rc::Rc<super::inline_players::InlineVideos>,
    /// MED1: playback speed multiplier, 0.5–2.0 (TGX `PlaybackSpeed*`;
    /// applied by the pitch-preserving tempo stretcher + the playback clock rate).
    pub(super) speed: f64,
    /// MED1: playback volume 0.0–1.0 (player volume); 0 is muted.
    pub(super) volume: f32,
    /// MED1: last non-zero volume, restored by the mute toggle.
    pub(super) unmuted_volume: f32,
    /// MED1: honest playback error for the active track — set instead of
    /// the old silent failure (e.g. unsupported format, no output device).
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

    pub(super) fn new(audio_output: &super::audio::SharedOutput) -> Self {
        Self {
            player: Default::default(),
            playing_voice: None,
            playing_audio: None,
            pending_audio_play: None,
            playlist_menu: None,
            playlist_remove: None,
            pending_voice_play: None,
            audio: super::audio::AudioEngine::new(audio_output.clone()),
            clock: None,
            path: None,
            seek_slider: None,
            seek_scrubbing: false,
            seek_preview_secs: None,
            tick: false,
            positions: HashMap::new(),
            sticker_playback: Default::default(),
            emoji_playback: Default::default(),
            playing_animation: None,
            autoplayed_gifs: Default::default(),
            gif: None,
            gif_started_at: None,
            gif_decodes: HashMap::new(),
            pending_gif_play: None,
            pending_media_seek: None,
            inline_videos: Default::default(),
            speed: 1.0,
            volume: 1.0,
            unmuted_volume: 1.0,
            error: None,
        }
    }
}
