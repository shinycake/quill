//! Video chat UI: its window, dialogs, composer, pin and push-to-talk.
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use std::collections::HashMap;
use std::time::Instant;

pub(crate) struct GroupCallUi {
    /// Phase C3a: voice-chat title rename dialog (`setVideoChatTitle`).
    pub(super) title_dialog: Option<GroupCallTitleDialog>,
    /// Starting a video chat: leave-current, create and schedule boxes
    /// (`dialogs/group_call_create.rs`).
    pub(super) start_dialog: Option<super::dialogs::group_call_create::GroupCallCreate>,
    /// The last `createVideoChat` sent, to match a server failure.
    pub(super) create_request: Option<quill::ids::RequestId>,
    /// The call window's Settings box (`group_call_settings.rs`).
    pub(super) settings_open: bool,
    /// Phase C2h: in-call chat composer (sendGroupCallMessage).
    pub(super) composer: Entity<TextareaState>,
    /// Phase C2f: voice-chat invite picker overlay (contacts list).
    pub(super) invite_open: bool,
    /// "Share Invite Link" box: asked for (it shows once both links are
    /// in), its "Users with this link can speak" checkbox, and whether
    /// the link was just copied.
    pub(super) share_open: bool,
    pub(super) share_speaker: bool,
    pub(super) share_copied: bool,
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
    /// Release-delay slider in Settings > Calls (`ui/ptt_delay.rs`).
    pub(super) ptt_delay_slider: Option<Entity<gpui_kit::component::slider::SliderState>>,
    /// System-wide push-to-talk hook, live only while joined with PTT on.
    pub(super) global_ptt: quill::calls::ptt_global::PlatformController,
    pub(super) global_ptt_polling: bool,
    /// Ends the task listening to the hook's key edges (set when the hook
    /// is torn down or replaced, so no thread waits on a dead channel).
    pub(super) global_ptt_listener_stop: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    /// Locally pinned video tile of the group call (tdesktop viewport pin).
    pub(super) pin: quill::calls::tile_pin::TilePin,
    /// Your own microphone level on the mute button: the halo follows
    /// the driver's level over `LEVEL_ANIMATION_MS` (tdesktop's blobs).
    /// `level_seen` is the last driver level the animation was aimed
    /// at.
    pub(super) level_anim: quill::calls::audio_level::LevelAnimation,
    pub(super) level_seen: f32,
    /// Each sounding member row's eased level, by audio source
    /// (`group_call_speaker.rs`).
    pub(super) row_levels: HashMap<i32, quill::calls::audio_level::LevelAnimation>,
    /// The video tiles' live players, by feed (`(call, user, screen)`,
    /// or your camera), for the tracked call only.
    pub(super) players:
        HashMap<quill::calls::live_video::LiveKey, super::video_player::VideoPlayer>,
    /// The tiles' live overlay clocks (opening spinner), by feed.
    pub(super) tile_overlays:
        HashMap<quill::calls::live_video::LiveKey, super::video_player::controls::LiveOverlayClock>,
    /// The stage's volume slider for the pinned participant.
    pub(super) stage_volume: Option<super::group_call_stage_volume::StageVolume>,
    /// The live stream layout (RTMP): picture, placeholder and controls.
    pub(super) stream: super::group_call_stream_layout::StreamWindow,
    /// The "Stream with other apps" box (`dialogs/rtmp_stream.rs`).
    pub(super) rtmp_start: super::dialogs::rtmp_stream::RtmpStartUi,
    /// The live Picture-in-Picture of a stream or a live story
    /// (`calls::live_pip`).
    pub(super) live_pip: Option<super::calls::live_pip::LivePipLink>,
}

impl GroupCallUi {
    pub(super) fn new(composer: Entity<TextareaState>) -> Self {
        Self {
            composer,
            title_dialog: None,
            start_dialog: None,
            create_request: None,
            settings_open: false,
            invite_open: false,
            share_open: false,
            share_speaker: true,
            share_copied: false,
            window: None,
            window_opening: false,
            window_closed_by_user: None,
            chat_shown: false,
            ptt: quill::calls::ptt::PushToTalk::new(),
            ptt_clock: std::time::Instant::now(),
            ptt_capture: false,
            ptt_delay_slider: None,
            global_ptt: Default::default(),
            global_ptt_polling: false,
            global_ptt_listener_stop: None,
            pin: quill::calls::tile_pin::TilePin::default(),
            tile_overlays: HashMap::new(),
            stage_volume: None,
            level_anim: quill::calls::audio_level::LevelAnimation::new(
                0.0,
                quill::calls::audio_level::LEVEL_ANIMATION_MS,
            ),
            level_seen: 0.0,
            row_levels: HashMap::new(),
            players: HashMap::new(),
            stream: Default::default(),
            rtmp_start: Default::default(),
            live_pip: None,
        }
    }
}
