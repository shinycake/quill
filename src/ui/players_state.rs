//! The app's players: the audio engine, the shared sticker and emoji
//! players, inline and userpic videos, and the speed and volume settings.
//! They outlive the account on screen; what plays *for* that account (the
//! track, the GIF, the seek bar) is `playback_state::PlaybackUi`, on
//! `OnScreen`.
#![deny(clippy::indexing_slicing)]

pub(crate) struct PlayersUi {
    /// Voice and music output.
    pub(super) audio: super::audio::AudioEngine,
    /// The task that lands finished engine builds is running.
    pub(super) builds_watched: bool,
    /// Shared across every animated sticker in the history, the panel and
    /// the viewer (`sticker_player`).
    pub(super) stickers: std::rc::Rc<super::sticker_player::Player>,
    /// Animated emoji playing in the history (`animated_emoji`), keyed by
    /// chat and message ids, so filled for one account at a time.
    pub(super) emoji_plays:
        std::cell::RefCell<quill::ids::OwnedBy<quill::animated_emoji::EmojiPlays>>,
    /// Inline video players in history rows (`inline_players`), keyed by
    /// chat and message ids under the account they play for
    /// (`InlineVideos::own`).
    pub(super) inline_videos: std::rc::Rc<super::inline_players::InlineVideos>,
    /// Animated profile photos (`userpic_videos`).
    pub(super) userpic_videos: std::rc::Rc<super::userpic_videos::UserpicVideos>,
    /// Playback speed for voice and video messages (tdesktop's 0.5–2.5x).
    pub(super) speed: f64,
    /// Output volume, 0.0 to 1.0.
    pub(super) volume: f32,
    /// The volume before mute, for the unmute click.
    pub(super) unmuted_volume: f32,
}

impl PlayersUi {
    pub(super) fn new(audio_output: &super::audio::SharedOutput) -> Self {
        Self {
            audio: super::audio::AudioEngine::new(audio_output.clone()),
            builds_watched: false,
            stickers: std::rc::Rc::new(super::sticker_player::Player::new()),
            emoji_plays: std::cell::RefCell::default(),
            inline_videos: Default::default(),
            userpic_videos: Default::default(),
            speed: 1.0,
            volume: 1.0,
            unmuted_volume: 1.0,
        }
    }
}
