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
        /// What the window was sized for: another layout reopens it.
        layout: CallLayout,
    },
}

/// The layout a call window was built for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CallLayout {
    /// Members and tiles.
    Voice,
    /// The live stream's picture.
    Stream,
}

impl CallLayout {
    pub(crate) fn of(stream: bool) -> Self {
        if stream { Self::Stream } else { Self::Voice }
    }
}

impl CallWindow {
    /// The open window, if there is one.
    pub(crate) fn handle(self) -> Option<AnyWindowHandle> {
        match self {
            CallWindow::Open { handle, .. } => Some(handle),
            CallWindow::Closed | CallWindow::Opening => None,
        }
    }

    /// The layout of the open window, if there is one.
    pub(crate) fn layout(self) -> Option<CallLayout> {
        match self {
            CallWindow::Open { layout, .. } => Some(layout),
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
    /// Push-to-talk state for the group call window, its clock origin,
    /// and the release delay's timer lives in the stage that waits for it.
    pub(super) ptt: quill::calls::ptt::PushToTalk<Task<()>>,
    pub(super) ptt_clock: Instant,
    /// System-wide push-to-talk hook, live only while joined with PTT on.
    pub(super) global_ptt: GlobalPtt,
    /// Your own microphone level on the mute button: the halo follows
    /// the driver's level over `LEVEL_ANIMATION_MS` (tdesktop's blobs).
    /// `level_seen` is the last driver level the animation was aimed
    /// at.
    pub(super) level_anim: quill::calls::audio_level::LevelAnimation,
    pub(super) level_seen: f32,
    pub(super) panel_scroll: ScrollHandle,
    pub(super) rows_place: super::group_call_members_view::RowsPlace,
    /// The video tiles' live players, by feed (`(call, user, screen)`,
    /// or your camera), for the tracked call only.
    pub(super) players:
        HashMap<quill::calls::live_video::LiveKey, super::video_player::VideoPlayer>,
    /// The tiles' live overlay clocks (opening spinner), by feed.
    pub(super) tile_overlays:
        HashMap<quill::calls::live_video::LiveKey, super::video_player::controls::LiveOverlayClock>,
    /// The live stream layout (RTMP): picture, placeholder and controls.
    pub(super) stream: super::group_call_stream_layout::StreamWindow,
    /// The live Picture-in-Picture of a stream or a live story
    /// (`calls::live_pip`).
    pub(super) live_pip: Option<super::calls::live_pip::LivePipLink>,
}

/// What the call window shows of one call: the box over it, the in-call
/// chat panel, each sounding member row's eased level (by audio source,
/// `group_call_speaker.rs`), the stage's volume slider and the pinned tile.
/// Group call ids are numbered per client, so it names its call and another
/// call, or none, replaces it whole (`sync_group_call_boxes`); a new
/// account on screen brings a new screen with none.
pub(super) struct CallView {
    call: i32,
    pub(super) open: CallBox,
    pub(super) chat_shown: bool,
    pub(super) row_levels: HashMap<i32, quill::calls::audio_level::LevelAnimation>,
    pub(super) stage_volume: Option<super::group_call_stage_volume::StageVolume>,
    /// Locally pinned video tile of the group call (tdesktop viewport pin).
    pub(super) pin: quill::calls::tile_pin::TilePin,
}

impl CallView {
    fn new(call: i32) -> Self {
        Self {
            call,
            open: CallBox::None,
            chat_shown: false,
            row_levels: HashMap::new(),
            stage_volume: None,
            pin: Default::default(),
        }
    }
}

impl GroupCallScreen {
    /// The view of `call`, made afresh when it was another call's.
    pub(super) fn view_of(&mut self, call: i32) -> &mut CallView {
        if self.view.as_ref().is_some_and(|view| view.call != call) {
            self.view = None;
        }
        self.view.get_or_insert_with(|| CallView::new(call))
    }

    /// The view of `call`, if it has one.
    pub(super) fn view_for(&self, call: i32) -> Option<&CallView> {
        self.view.as_ref().filter(|view| view.call == call)
    }
}

impl QuillApp {
    /// The tracked call's view, made when the call has none yet.
    pub(super) fn call_view_mut(&mut self) -> Option<&mut CallView> {
        let id = self.tracked_group_call_id()?;
        Some(self.group_call_screen.view_of(id))
    }

    /// The tracked call's pinned tile (none without a view).
    pub(super) fn call_pin(&self) -> quill::calls::tile_pin::TilePin {
        self.call_view().map(|view| view.pin).unwrap_or_default()
    }

    /// The tracked call's view, if it has one.
    pub(super) fn call_view(&self) -> Option<&CallView> {
        self.group_call_screen
            .view_for(self.tracked_group_call_id()?)
    }
}

/// What the account on screen has open in its video chat: the rename and
/// Settings boxes, the in-call composer's text, the pinned tile, the call
/// whose window you closed and the stream you asked to join. Group call
/// ids are numbered per client, so a value kept app-wide would match the
/// next account's call of the same id. The call itself does not outlive
/// the screen: `retire_live_to_background` closes a client in a call. A
/// field of `OnScreen`.
pub(crate) struct GroupCallScreen {
    /// What the window shows of the tracked call, the box over it among it.
    pub(super) view: Option<CallView>,
    /// Phase C2h: in-call chat composer (sendGroupCallMessage).
    pub(super) composer: Entity<TextareaState>,
    /// The call whose window you closed: it stays closed until the call
    /// bar reopens it.
    pub(super) window_closed_by_user: Option<i32>,
    /// The stream you asked to watch and where that stands.
    pub(super) stream_watch: StreamWatch,
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

    /// A join was sent: the Join button waits on it. A wish for a stream
    /// stays, with this join kept under it (`want_stream` does the same the
    /// other way round).
    pub(super) fn start_join(&mut self, joining: StreamJoining) {
        self.stream_watch = match std::mem::replace(&mut self.stream_watch, StreamWatch::Idle) {
            StreamWatch::Wanted {
                intent,
                tracked_then,
                ..
            } => StreamWatch::Wanted {
                intent,
                tracked_then,
                joining: Some(joining),
            },
            StreamWatch::Idle | StreamWatch::Joining(_) => StreamWatch::Joining(joining),
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
            view: None,
            composer,
            window_closed_by_user: None,
            stream_watch: StreamWatch::Idle,
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

/// The one box open over the call window (tdesktop opens one layer at a
/// time): opening one replaces the other, and closing, a call change
/// (`sync_group_call_boxes`) and the call having no links to share all
/// leave to `None`. It lives in the [`CallView`] of the call it acts on.
pub(crate) enum CallBox {
    None,
    /// Phase C3a: voice-chat title rename dialog (`setVideoChatTitle`).
    Rename(GroupCallTitleDialog),
    /// The call window's Settings box (`group_call_settings.rs`).
    Settings,
    /// Phase C2f: voice-chat invite picker overlay (contacts list).
    Invite,
    /// "Share Invite Link" box: asked for (it shows once both links are
    /// in), its "Users with this link can speak" checkbox, and whether
    /// the link was just copied.
    Share {
        speaker: bool,
        copied: bool,
    },
    /// A box over the live stream window (`group_call_stream_boxes.rs`).
    Stream(super::group_call_stream_boxes::StreamOverlay),
}

impl QuillApp {
    /// The box open over the tracked call's window.
    pub(super) fn call_box(&self) -> Option<&CallBox> {
        self.call_view().map(|view| &view.open)
    }

    /// Open `open` over the tracked call's window, replacing the box there;
    /// without a tracked call there is nothing to open it for. Whether it
    /// opened.
    pub(super) fn open_call_box(&mut self, open: CallBox) -> bool {
        match self.call_view_mut() {
            Some(view) => {
                view.open = open;
                true
            }
            None => false,
        }
    }

    pub(super) fn close_call_box(&mut self) {
        if let Some(view) = self.group_call_screen.view.as_mut() {
            view.open = CallBox::None;
        }
    }

    pub(super) fn invite_open(&self) -> bool {
        matches!(self.call_box(), Some(CallBox::Invite))
    }

    pub(super) fn share_open(&self) -> bool {
        matches!(self.call_box(), Some(CallBox::Share { .. }))
    }

    pub(super) fn settings_open(&self) -> bool {
        matches!(self.call_box(), Some(CallBox::Settings))
    }

    pub(super) fn title_dialog(&self) -> Option<&GroupCallTitleDialog> {
        match self.call_box() {
            Some(CallBox::Rename(dialog)) => Some(dialog),
            _ => None,
        }
    }

    /// The rename box, taken out of the window (put back with
    /// `open_call_box`).
    pub(super) fn take_title_dialog(&mut self) -> Option<GroupCallTitleDialog> {
        let view = self.group_call_screen.view.as_mut()?;
        match std::mem::replace(&mut view.open, CallBox::None) {
            CallBox::Rename(dialog) => Some(dialog),
            other => {
                view.open = other;
                None
            }
        }
    }

    pub(super) fn stream_overlay(&self) -> Option<super::group_call_stream_boxes::StreamOverlay> {
        match self.call_box() {
            Some(CallBox::Stream(overlay)) => Some(*overlay),
            _ => None,
        }
    }

    /// The share box's "can speak" checkbox (the speaker link is shown).
    pub(super) fn share_speaker(&self) -> bool {
        matches!(self.call_box(), Some(CallBox::Share { speaker: true, .. }))
    }

    pub(super) fn share_copied(&self) -> bool {
        matches!(self.call_box(), Some(CallBox::Share { copied: true, .. }))
    }

    pub(super) fn set_share_speaker(&mut self, on: bool) {
        if let Some(view) = self.group_call_screen.view.as_mut()
            && let CallBox::Share { speaker, copied } = &mut view.open
        {
            *speaker = on;
            *copied = false;
        }
    }

    pub(super) fn mark_share_copied(&mut self) {
        if let Some(view) = self.group_call_screen.view.as_mut()
            && let CallBox::Share { copied, .. } = &mut view.open
        {
            *copied = true;
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
        // The box, the pin, the in-call chat, the member levels and the
        // stage volume are the last call's: the view goes with it, so a
        // rename typed for one call cannot rename the next.
        if self
            .group_call_screen
            .view
            .as_ref()
            .is_some_and(|view| Some(view.call) != id)
        {
            self.group_call_screen.view = None;
        }
        // The rename, Settings and screen privacy boxes are drawn in the
        // call window: with that window closed they would show nowhere.
        if id.is_some() && self.group_call_screen.window_closed_by_user == id {
            if matches!(
                self.call_box(),
                Some(CallBox::Rename(_) | CallBox::Settings)
            ) {
                self.close_call_box();
            }
            if self.screen_share.chooser.privacy_for().map(|t| t.for_call)
                == Some(super::screen_share_state::ScreenFor::Group)
            {
                self.screen_share.close_privacy();
            }
        }
        if self.share_open() && call.is_some_and(|(_, generating, ready)| !generating && !ready) {
            self.close_call_box();
        }
    }
}

impl GroupCallBoxes {
    pub(super) fn new() -> Self {
        Self {
            start_dialog: None,
            create: VideoChatCreate::Idle,
            rtmp_start: Default::default(),
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
            ptt: quill::calls::ptt::PushToTalk::new(),
            ptt_clock: std::time::Instant::now(),
            global_ptt: GlobalPtt::default(),
            tile_overlays: HashMap::new(),
            level_anim: quill::calls::audio_level::LevelAnimation::new(
                0.0,
                quill::calls::audio_level::LEVEL_ANIMATION_MS,
            ),
            level_seen: 0.0,
            panel_scroll: ScrollHandle::new(),
            rows_place: Default::default(),
            players: HashMap::new(),
            stream: Default::default(),
            live_pip: None,
        }
    }
}
