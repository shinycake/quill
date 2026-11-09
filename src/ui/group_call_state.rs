//! Video chat UI: its window and push-to-talk (the app's), its boxes,
//! composer and pin (the screen's).
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::*;
use quill::lang::{keys, tr};
use std::collections::HashMap;
use std::time::Instant;

pub(crate) struct GroupCallUi {
    /// The voice / video chat window, as for 1:1 calls; the in-call
    /// chat shows under the members when asked. It follows the session
    /// on screen (`sync_group_call_window`), so it closes when the next
    /// account tracks no call.
    pub(super) window: Option<AnyWindowHandle>,
    pub(super) window_opening: bool,
    pub(super) chat_shown: bool,
    /// Push-to-talk state for the group call window, its clock origin,
    /// and whether Settings is waiting for the next key to bind.
    pub(super) ptt: quill::calls::ptt::PushToTalk,
    pub(super) ptt_clock: Instant,
    pub(super) ptt_capture: bool,
    /// System-wide push-to-talk hook, live only while joined with PTT on.
    pub(super) global_ptt: quill::calls::ptt_global::PlatformController,
    pub(super) global_ptt_polling: bool,
    /// Ends the task listening to the hook's key edges (set when the hook
    /// is torn down or replaced, so no thread waits on a dead channel).
    pub(super) global_ptt_listener_stop: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    pub(super) window_visible: bool,
    /// Your own microphone level on the mute button: the halo follows
    /// the driver's level over `LEVEL_ANIMATION_MS` (tdesktop's blobs).
    /// `level_seen` is the last driver level the animation was aimed
    /// at.
    pub(super) level_anim: quill::calls::audio_level::LevelAnimation,
    pub(super) level_seen: f32,
    /// Each sounding member row's eased level, by audio source
    /// (`group_call_speaker.rs`).
    pub(super) row_levels: HashMap<i32, quill::calls::audio_level::LevelAnimation>,
    pub(super) panel_scroll: ScrollHandle,
    pub(super) rows_place: super::group_call_members_view::RowsPlace,
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
    /// The live Picture-in-Picture of a stream or a live story
    /// (`calls::live_pip`).
    pub(super) live_pip: Option<super::calls::live_pip::LivePipLink>,
}

/// What the account on screen has open in its video chat: the rename and
/// Settings boxes, the in-call composer's text, the pinned tile, the call
/// whose window you closed and the stream you asked to join. Group call
/// ids are numbered per client, so a value kept app-wide would match the
/// next account's call of the same id. The call itself does not outlive
/// the screen: `retire_live_to_background` closes a client in a call. A
/// field of `OnScreen`.
pub(crate) struct GroupCallScreen {
    /// Phase C3a: voice-chat title rename dialog (`setVideoChatTitle`).
    pub(super) title_dialog: Option<GroupCallTitleDialog>,
    /// The call window's Settings box (`group_call_settings.rs`).
    pub(super) settings_open: bool,
    /// Phase C2h: in-call chat composer (sendGroupCallMessage).
    pub(super) composer: Entity<TextareaState>,
    /// The call whose window you closed: it stays closed until the call
    /// bar reopens it.
    pub(super) window_closed_by_user: Option<i32>,
    /// Locally pinned video tile of the group call (tdesktop viewport pin).
    pub(super) pin: quill::calls::tile_pin::TilePin,
    /// The stream you asked to watch, until it is tracked, with the call
    /// tracked when you asked (another call taking its place drops it).
    pub(super) stream_join: Option<(
        super::group_call_stream_layout::StreamJoinIntent,
        Option<i32>,
    )>,
    /// The stream call whose join was sent (the Join button waits): call
    /// ids repeat across clients, and the timeout below is a task of this
    /// screen, so a wait left over would never end on the next account.
    pub(super) stream_joining: Option<i32>,
    /// Gives up on a join that neither lands nor fails.
    pub(super) stream_join_timeout: Option<Task<()>>,
    /// The call the boxes above and [`GroupCallBoxes`]' invite and share
    /// boxes were opened for (`sync_group_call_boxes`): another call, or
    /// none, closes them, so a rename typed for one call cannot rename the
    /// next.
    pub(super) boxes_call: Option<i32>,
}

impl GroupCallScreen {
    pub(super) fn new(
        send_key_mode: quill::composer::SendKeyMode,
        window: &mut Window,
        cx: &mut Context<QuillApp>,
    ) -> Self {
        let submit_on_enter = send_key_mode == quill::composer::SendKeyMode::Enter;
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_AV_MESSAGE_THE_VOICE_CHAT))
                .auto_grow(1, 3)
                .submit_on_enter(submit_on_enter)
        });
        cx.subscribe_in(
            &composer,
            window,
            |this, state, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if quill::composer::should_send_on_enter(
                        quill::composer::enter_event_from_kit(*shift, *secondary, marked),
                        this.chat_prefs.send_key_mode,
                    ) {
                        this.send_group_call_message(window, cx);
                    }
                }
            },
        )
        .detach();
        Self {
            title_dialog: None,
            settings_open: false,
            composer,
            window_closed_by_user: None,
            pin: quill::calls::tile_pin::TilePin::default(),
            stream_join: None,
            stream_joining: None,
            stream_join_timeout: None,
            boxes_call: None,
        }
    }
}

/// Dropped with the screen: the rename box and the composer's text go,
/// and the pending join with them, so none can act on the next account's
/// call of the same id. The window itself closes on the next sync, when
/// the next account tracks no call.
impl super::on_screen::Farewell for GroupCallScreen {}

/// The boxes the account on screen opens around a video chat: starting
/// one, the invite picker, the share-link box. The call window and the
/// call ([`GroupCallUi`]) are the app's. A field of `OnScreen`.
pub(crate) struct GroupCallBoxes {
    /// Starting a video chat: leave-current, create and schedule boxes
    /// (`dialogs/group_call_create.rs`).
    pub(super) start_dialog: Option<super::dialogs::group_call_create::GroupCallCreate>,
    /// The last `createVideoChat` sent, to match a server failure.
    pub(super) create_request: Option<quill::ids::RequestId>,
    /// The "Stream with other apps" box (`dialogs/rtmp_stream.rs`).
    pub(super) rtmp_start: super::dialogs::rtmp_stream::RtmpStartUi,
    /// Phase C2f: voice-chat invite picker overlay (contacts list).
    pub(super) invite_open: bool,
    /// "Share Invite Link" box: asked for (it shows once both links are
    /// in), its "Users with this link can speak" checkbox, and whether
    /// the link was just copied.
    pub(super) share_open: bool,
    pub(super) share_speaker: bool,
    pub(super) share_copied: bool,
}

impl QuillApp {
    /// The video chat's boxes belong to the call they were opened for: when
    /// the tracked call changes or ends, the rename and Settings boxes, the
    /// invite picker, the share box and the pin go. A share box whose
    /// listener link failed is closed too: it is drawn only once the links
    /// are in, and an undrawn box still took the stream window's keys and
    /// Esc. Runs at every main frame and every call window frame.
    pub(super) fn sync_group_call_boxes(&mut self) {
        let call = self
            .session()
            .and_then(|session| session.calls.active_group_call.as_ref())
            .map(|call| {
                (
                    call.id,
                    call.share_links.generating,
                    call.share_links.ready(),
                )
            });
        let id = call.map(|(id, _, _)| id);
        let before = self.group_call_screen.boxes_call;
        self.group_call_screen.boxes_call = id;
        // Nothing recorded yet: nothing was opened for another call.
        if before.is_some() && before != id {
            self.group_call_screen.title_dialog = None;
            self.group_call_screen.settings_open = false;
            self.group_call_screen.pin = Default::default();
            self.group_call_boxes.invite_open = false;
            self.group_call_boxes.share_open = false;
            self.group_call_boxes.share_copied = false;
        }
        // The rename, Settings and screen privacy boxes are drawn in the
        // call window: with that window closed they would show nowhere.
        if id.is_some() && self.group_call_screen.window_closed_by_user == id {
            self.group_call_screen.title_dialog = None;
            self.group_call_screen.settings_open = false;
            if self.screen_share.privacy_for == Some(super::screen_share_state::ScreenFor::Group) {
                self.screen_share.privacy_for = None;
            }
        }
        if self.group_call_boxes.share_open
            && call.is_some_and(|(_, generating, ready)| !generating && !ready)
        {
            self.group_call_boxes.share_open = false;
        }
    }
}

impl GroupCallBoxes {
    pub(super) fn new() -> Self {
        Self {
            start_dialog: None,
            create_request: None,
            rtmp_start: Default::default(),
            invite_open: false,
            share_open: false,
            share_speaker: true,
            share_copied: false,
        }
    }
}

/// The RTMP box keeps its process in the session (`calls.rtmp_start`),
/// which goes to the background with the account: it ends here, or it
/// would come back on switch-back with the stream key hidden again.
impl super::on_screen::Farewell for GroupCallBoxes {
    fn farewell(
        app: &mut QuillApp,
        _: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<QuillApp>,
    ) {
        if app
            .session()
            .is_some_and(|session| session.calls.rtmp_start.is_some())
        {
            app.close_rtmp_start(cx);
        }
    }
}

impl GroupCallUi {
    pub(super) fn new() -> Self {
        Self {
            window: None,
            window_opening: false,
            chat_shown: false,
            ptt: quill::calls::ptt::PushToTalk::new(),
            ptt_clock: std::time::Instant::now(),
            ptt_capture: false,
            global_ptt: Default::default(),
            global_ptt_polling: false,
            global_ptt_listener_stop: None,
            window_visible: true,
            tile_overlays: HashMap::new(),
            stage_volume: None,
            level_anim: quill::calls::audio_level::LevelAnimation::new(
                0.0,
                quill::calls::audio_level::LEVEL_ANIMATION_MS,
            ),
            level_seen: 0.0,
            row_levels: HashMap::new(),
            panel_scroll: ScrollHandle::new(),
            rows_place: Default::default(),
            players: HashMap::new(),
            stream: Default::default(),
            live_pip: None,
        }
    }
}
