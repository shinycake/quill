//! Video chat UI: its window, dialogs, composer, pin and push-to-talk.

use super::*;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use std::time::Instant;

pub(crate) struct GroupCallUi {
    /// Phase C3a: voice-chat title rename dialog (`setVideoChatTitle`).
    pub(super) title_dialog: Option<GroupCallTitleDialog>,
    /// Phase C2h: `createVideoChat` start/schedule dialog.
    pub(super) start_dialog: Option<GroupCallStartDialog>,
    /// Phase C2h: in-call chat composer (sendGroupCallMessage).
    pub(super) composer: Entity<TextareaState>,
    /// Phase C2f: voice-chat invite picker overlay (contacts list).
    pub(super) invite_open: bool,
    /// The voice / video chat window, as for 1:1 calls; the in-call
    /// chat shows under the members when asked.
    pub(super) window: Option<AnyWindowHandle>,
    pub(super) window_opening: bool,
    pub(super) window_closed_by_user: Option<i32>,
    pub(super) chat_shown: bool,
    /// Push-to-talk state for the group call window, its clock origin,
    /// and whether Settings is waiting for the next key to bind.
    pub(super) ptt: quill::calls::ptt::PushToTalk,
    pub(super) ptt_clock: Instant,
    pub(super) ptt_capture: bool,
    /// System-wide push-to-talk hook, live only while joined with PTT on.
    pub(super) global_ptt: quill::calls::ptt_global::PlatformController,
    pub(super) global_ptt_polling: bool,
    /// Locally pinned video tile of the group call (tdesktop viewport pin).
    pub(super) pin: quill::calls::tile_pin::TilePin,
    /// Your own microphone level on the mute button: the halo follows
    /// the driver's level over `LEVEL_ANIMATION_MS` (tdesktop's blobs).
    /// `level_seen` is the last driver level the animation was aimed
    /// at; `demo_level` stands in for the driver in screenshot demos.
    pub(super) level_anim: quill::calls::audio_level::LevelAnimation,
    pub(super) level_seen: f32,
    pub(super) demo_level: Option<f32>,
}

impl GroupCallUi {
    pub(super) fn new(composer: Entity<TextareaState>) -> Self {
        Self {
            composer,
            title_dialog: None,
            start_dialog: None,
            invite_open: false,
            window: None,
            window_opening: false,
            window_closed_by_user: None,
            chat_shown: false,
            ptt: quill::calls::ptt::PushToTalk::new(),
            ptt_clock: std::time::Instant::now(),
            ptt_capture: false,
            global_ptt: Default::default(),
            global_ptt_polling: false,
            pin: quill::calls::tile_pin::TilePin::default(),
            level_anim: quill::calls::audio_level::LevelAnimation::new(
                0.0,
                quill::calls::audio_level::LEVEL_ANIMATION_MS,
            ),
            level_seen: 0.0,
            demo_level: None,
        }
    }
}
