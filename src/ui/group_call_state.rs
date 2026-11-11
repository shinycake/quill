//! Video chat UI: its window and push-to-talk (the app's), its boxes,
//! composer and pin (the screen's).
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::*;
use quill::lang::{keys, tr};
use std::collections::HashMap;
use std::time::Instant;

/// Where the call window stands. A window being built is `Opening` (a second
/// open waits for it); it leaves that stage when the window exists (`Open`)
/// or fails to build (`Closed`), and `Open` ends when the window goes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum CallWindow {
    #[default]
    Closed,
    Opening,
    Open {
        handle: AnyWindowHandle,
        /// The window is on screen (not minimized or hidden).
        visible: bool,
    },
}

impl CallWindow {
    /// The open window, if there is one.
    pub(crate) fn handle(self) -> Option<AnyWindowHandle> {
        match self {
            CallWindow::Open { handle, .. } => Some(handle),
            CallWindow::Closed | CallWindow::Opening => None,
        }
    }

    /// Open and on screen.
    pub(crate) fn is_visible(self) -> bool {
        matches!(self, CallWindow::Open { visible: true, .. })
    }

    /// Whether `handle` is the open window.
    pub(crate) fn is(self, handle: AnyWindowHandle) -> bool {
        self.handle() == Some(handle)
    }
}

/// The system-wide push-to-talk hook and the two tasks that serve it. A
/// task handle is the stage: holding it is the stage, dropping it (the hook
/// torn down or replaced, the permission granted) is the way out, so no flag
/// and no stop signal are kept in step with the hook.
#[derive(Default)]
pub(crate) struct GlobalPtt {
    pub(super) hook: quill::calls::ptt_global::PlatformController,
    /// Forwards the running hook's key edges to the app; dropped with the
    /// hook, so no thread waits on a dead channel.
    pub(super) listener: Option<Task<()>>,
    /// The re-check of a missing Input Monitoring permission, one tick away.
    pub(super) permission_poll: Option<Task<()>>,
}

pub(crate) struct GroupCallUi {
    /// The voice / video chat window, as for 1:1 calls; the in-call
    /// chat shows under the members when asked. It follows the session
    /// on screen (`sync_group_call_window`), so it closes when the next
    /// account tracks no call.
    pub(super) window: CallWindow,
    pub(super) chat_shown: bool,
    /// Push-to-talk state for the group call window, its clock origin,
    /// and whether Settings is waiting for the next key to bind.
    pub(super) ptt: quill::calls::ptt::PushToTalk,
    pub(super) ptt_clock: Instant,
    pub(super) ptt_capture: bool,
    /// System-wide push-to-talk hook, live only while joined with PTT on.
    pub(super) global_ptt: GlobalPtt,
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
    /// The stream you asked to watch and where that stands.
    pub(super) stream_watch: StreamWatch,
    /// The call the boxes above were opened for (`sync_group_call_boxes`):
    /// another call, or none, closes them, so a rename typed for one call
    /// cannot rename the next. The invite and share boxes
    /// ([`GroupCallBoxes`]) carry their own call in [`CallBox`].
    pub(super) boxes_call: Option<i32>,
    /// The call the composer's unsent text was typed for: another call
    /// clears it where a window is at hand (`sync_group_call_composer`).
    pub(super) composer_call: Option<i32>,
}

/// A stream join on its way: the call and the task that gives up on it
/// when it neither lands nor fails. Dropping it ends the wait.
pub(super) struct StreamJoining {
    pub(super) call: i32,
    _timeout: Task<()>,
}

impl StreamJoining {
    pub(super) fn new(call: i32, timeout: Task<()>) -> Self {
        Self {
            call,
            _timeout: timeout,
        }
    }
}

/// The stream you asked to watch, one stage at a time. The wish waits for
/// its call to be tracked (`Wanted`, with the call tracked when you asked:
/// another call taking its place drops it); once the join is sent, the Join
/// button waits on `Joining`, whose task gives up on it. Call ids repeat
/// across clients and the value belongs to this screen, so a wait left over
/// never ends up on the next account. Every stage leaves to `Idle`: the
/// call is tracked and joined, the fetch failed, the window closed, the
/// join landed or timed out.
pub(super) enum StreamWatch {
    Idle,
    Wanted {
        intent: super::group_call_stream_layout::StreamJoinIntent,
        tracked_then: Option<i32>,
        /// A join already sent, still waiting for its answer: a new wish
        /// does not drop it, so its timeout and failure still land.
        joining: Option<StreamJoining>,
    },
    Joining(StreamJoining),
}

impl GroupCallScreen {
    /// The call whose join is on its way, if any.
    pub(super) fn joining_call(&self) -> Option<i32> {
        match &self.stream_watch {
            StreamWatch::Joining(joining)
            | StreamWatch::Wanted {
                joining: Some(joining),
                ..
            } => Some(joining.call),
            StreamWatch::Idle | StreamWatch::Wanted { joining: None, .. } => None,
        }
    }

    /// The stream you asked for and the call tracked when you asked, until
    /// it is tracked.
    pub(super) fn stream_wish(
        &self,
    ) -> Option<(
        super::group_call_stream_layout::StreamJoinIntent,
        Option<i32>,
    )> {
        match &self.stream_watch {
            StreamWatch::Wanted {
                intent,
                tracked_then,
                ..
            } => Some((*intent, *tracked_then)),
            StreamWatch::Idle | StreamWatch::Joining(_) => None,
        }
    }

    /// Drop the wish; a join already sent keeps waiting for its answer.
    pub(super) fn clear_stream_wish(&mut self) {
        self.stream_watch = match std::mem::replace(&mut self.stream_watch, StreamWatch::Idle) {
            StreamWatch::Wanted {
                joining: Some(joining),
                ..
            } => StreamWatch::Joining(joining),
            StreamWatch::Wanted { joining: None, .. } => StreamWatch::Idle,
            other => other,
        };
    }

    /// Stop waiting on a sent join; a wish not yet consumed stays.
    pub(super) fn clear_stream_joining(&mut self) {
        self.stream_watch = match std::mem::replace(&mut self.stream_watch, StreamWatch::Idle) {
            StreamWatch::Joining(_) => StreamWatch::Idle,
            StreamWatch::Wanted {
                intent,
                tracked_then,
                ..
            } => StreamWatch::Wanted {
                intent,
                tracked_then,
                joining: None,
            },
            StreamWatch::Idle => StreamWatch::Idle,
        };
    }

    /// Ask for a stream. A join already on its way stays alive under the
    /// new wish.
    pub(super) fn want_stream(
        &mut self,
        intent: super::group_call_stream_layout::StreamJoinIntent,
        tracked_then: Option<i32>,
    ) {
        let joining = match std::mem::replace(&mut self.stream_watch, StreamWatch::Idle) {
            StreamWatch::Joining(joining)
            | StreamWatch::Wanted {
                joining: Some(joining),
                ..
            } => Some(joining),
            StreamWatch::Idle | StreamWatch::Wanted { joining: None, .. } => None,
        };
        self.stream_watch = StreamWatch::Wanted {
            intent,
            tracked_then,
            joining,
        };
    }

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
            stream_watch: StreamWatch::Idle,
            boxes_call: None,
            composer_call: None,
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
    /// The last `createVideoChat` sent, until its answer.
    pub(super) create: VideoChatCreate,
    /// The "Stream with other apps" box (`dialogs/rtmp_stream.rs`).
    pub(super) rtmp_start: super::dialogs::rtmp_stream::RtmpStartUi,
    /// The invite picker or the share box over the call, if one is open.
    pub(super) call_box: CallBox,
}

/// The `createVideoChat` the boxes wait on. `Sent` leaves on its own answer:
/// the new call (`Session::answered_creates`) or the server's refusal
/// (`failed_creates`); a create that was not sent is `Idle`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum VideoChatCreate {
    #[default]
    Idle,
    Sent(quill::ids::RequestId),
}

/// The overlay open over the call window, with the call (tracked when it
/// opened) it acts on. Opening one replaces the other, closing, a call
/// change (`keep_for`) and the call having no links to share all leave to
/// `None`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CallBox {
    #[default]
    None,
    /// Phase C2f: voice-chat invite picker overlay (contacts list).
    Invite { call: Option<i32> },
    /// "Share Invite Link" box: asked for (it shows once both links are
    /// in), its "Users with this link can speak" checkbox, and whether
    /// the link was just copied.
    Share {
        call: Option<i32>,
        speaker: bool,
        copied: bool,
    },
}

impl GroupCallBoxes {
    pub(super) fn invite_open(&self) -> bool {
        matches!(self.call_box, CallBox::Invite { .. })
    }

    pub(super) fn share_open(&self) -> bool {
        matches!(self.call_box, CallBox::Share { .. })
    }

    /// The share box's "can speak" checkbox (the speaker link is shown).
    pub(super) fn share_speaker(&self) -> bool {
        matches!(self.call_box, CallBox::Share { speaker: true, .. })
    }

    pub(super) fn share_copied(&self) -> bool {
        matches!(self.call_box, CallBox::Share { copied: true, .. })
    }

    pub(super) fn set_share_speaker(&mut self, on: bool) {
        if let CallBox::Share {
            speaker, copied, ..
        } = &mut self.call_box
        {
            *speaker = on;
            *copied = false;
        }
    }

    pub(super) fn mark_share_copied(&mut self) {
        if let CallBox::Share { copied, .. } = &mut self.call_box {
            *copied = true;
        }
    }

    pub(super) fn close_call_box(&mut self) {
        self.call_box = CallBox::None;
    }

    /// A box opened for another call than `tracked` closes: it must not
    /// act on the next call.
    pub(super) fn keep_for(&mut self, tracked: Option<i32>) {
        let opened_for = match self.call_box {
            CallBox::None => return,
            CallBox::Invite { call } | CallBox::Share { call, .. } => call,
        };
        if opened_for != tracked {
            self.call_box = CallBox::None;
        }
    }
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
                    call.share_links.generating(),
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
            self.group_call.stream.close_boxes();
            // The in-call chat and the member levels are the last call's.
            self.group_call.chat_shown = false;
            self.group_call.row_levels.clear();
        }
        self.group_call_boxes.keep_for(id);
        // The rename, Settings and screen privacy boxes are drawn in the
        // call window: with that window closed they would show nowhere.
        if id.is_some() && self.group_call_screen.window_closed_by_user == id {
            self.group_call_screen.title_dialog = None;
            self.group_call_screen.settings_open = false;
            if self.screen_share.privacy_for == Some(super::screen_share_state::ScreenFor::Group) {
                self.screen_share.privacy_for = None;
            }
        }
        if self.group_call_boxes.share_open()
            && call.is_some_and(|(_, generating, ready)| !generating && !ready)
        {
            self.group_call_boxes.close_call_box();
        }
    }
}

impl GroupCallBoxes {
    pub(super) fn new() -> Self {
        Self {
            start_dialog: None,
            create: VideoChatCreate::Idle,
            rtmp_start: Default::default(),
            call_box: CallBox::None,
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
            window: CallWindow::Closed,
            chat_shown: false,
            ptt: quill::calls::ptt::PushToTalk::new(),
            ptt_clock: std::time::Instant::now(),
            ptt_capture: false,
            global_ptt: GlobalPtt::default(),
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
