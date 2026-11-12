//! The app's players: the audio engine, the shared sticker and emoji
//! players, inline and userpic videos, and the speed and volume settings.
//! They outlive the account on screen; what plays *for* that account (the
//! track, the GIF, the seek bar) is `playback_state::PlaybackUi`, on
//! `OnScreen`.
#![deny(clippy::indexing_slicing)]

/// The task that lands finished engine builds: not started, or running
/// (the app's, for as long as it lives).
pub(crate) enum BuildsWatch {
    Idle,
    Watching { _task: gpui_kit::Task<()> },
}

impl BuildsWatch {
    pub(super) fn is_watching(&self) -> bool {
        matches!(self, Self::Watching { .. })
    }
}

pub(crate) struct PlayersUi {
    /// Voice and music output.
    pub(super) audio: super::audio::AudioEngine,
    /// The task that lands finished engine builds.
    pub(super) builds: BuildsWatch,
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
    /// Output volume (`Volume`).
    pub(super) volume: Volume,
}

/// Output volume: a level, or muted with the level the unmute click brings
/// back. One value, so the restore level cannot go stale beside the level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Volume {
    /// Audible, 0.01 to 1.0.
    Level(f32),
    /// Silent (a level below 0.01, or the mute click).
    Muted { restore: f32 },
}

impl Volume {
    /// The quietest audible level; anything below is mute.
    const FLOOR: f32 = 0.01;

    /// What the output plays at, 0.0 to 1.0.
    pub(crate) fn level(self) -> f32 {
        match self {
            Self::Level(level) => level,
            Self::Muted { .. } => 0.0,
        }
    }

    pub(crate) fn is_muted(self) -> bool {
        matches!(self, Self::Muted { .. })
    }

    /// The level before mute (or the level now): where "unmute" and a
    /// Picture-in-Picture window opening muted start.
    pub(crate) fn last_audible(self) -> f32 {
        match self {
            Self::Level(level) => level,
            Self::Muted { restore } if restore >= Self::FLOOR => restore,
            Self::Muted { .. } => 1.0,
        }
    }

    /// The slider or a key set `level` (not finite: unchanged). A level
    /// below the floor is mute, keeping the level it left.
    pub(crate) fn set(self, level: f32) -> Self {
        if !level.is_finite() {
            return self;
        }
        let level = level.clamp(0.0, 1.0);
        if level < Self::FLOOR {
            Self::Muted {
                restore: self.last_audible(),
            }
        } else {
            Self::Level(level)
        }
    }

    /// The mute button.
    pub(crate) fn toggled(self) -> Self {
        match self {
            Self::Level(level) => Self::Muted { restore: level },
            Self::Muted { .. } => Self::Level(self.last_audible()),
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "players_state_tests.rs"]
mod tests;

impl PlayersUi {
    /// What the output plays at, 0.0 to 1.0.
    pub(super) fn volume(&self) -> f32 {
        self.volume.level()
    }

    pub(super) fn new(audio_output: &super::audio::SharedOutput) -> Self {
        Self {
            audio: super::audio::AudioEngine::new(audio_output.clone()),
            builds: BuildsWatch::Idle,
            stickers: std::rc::Rc::new(super::sticker_player::Player::new()),
            emoji_plays: std::cell::RefCell::default(),
            inline_videos: Default::default(),
            userpic_videos: Default::default(),
            speed: 1.0,
            volume: Volume::Level(1.0),
        }
    }
}
