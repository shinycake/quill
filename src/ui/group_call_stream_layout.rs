//! The live stream window: a channel's RTMP stream in the group call
//! window, laid out as tdesktop's `Calls::Group::Panel` does for
//! `_call->rtmp()` (forced `PanelMode::Wide`, calls_group_panel.cpp and
//! calls.style):
//!
//! - The stream fills the window: inset 9 px with a 24 px title band in a
//!   normal window (`groupCallNarrowSkip`, `groupCallWideVideoTop`), the
//!   whole window full screen or maximized. The picture is fitted without
//!   cropping on `groupCallMembersBg` (black full screen), corners rounded
//!   6 px except full screen. No members list, no camera, screen or
//!   microphone buttons.
//! - Title row: the call's title (or the chat's name), a bullet, the red
//!   LIVE pill and "{count} viewers" (`lng_group_call_rtmp_viewers`, at
//!   least 1), placed by [`title_layout`]. Full screen it slides with the
//!   controls on a rounded backdrop.
//! - Bottom bar: the live controller (`VideoController::live`) in its bar
//!   layout (`updateButtonsGeometry`, wide RTMP branch): the "…" menu for
//!   managers or Settings for viewers, mute, the round button that
//!   toggles full screen, Leave, on a rounded `groupCallBg` backdrop at
//!   80 %. Keys: Space does nothing, M mutes, F toggles full screen, Esc
//!   leaves it (`live_key_action`).
//! - The picture is a live player in its own view
//!   (`calls::stream_view`), under this layout, so a frame redraws only
//!   the picture. Before the first picture it shows the chat's photo
//!   blurred (radius 15), or the chat's initials without a photo. Nobody
//!   broadcasting: a rounded label a third of the way down
//!   (`setupEmptyRtmp`), with the admin text when the call knows its
//!   RTMP server URL.
//! - When the controls show, and when the cursor hides with them full
//!   screen: see [`ControlsFade`].
//! - Not joined (Quill only: tdesktop never shows the panel unjoined):
//!   the label holds a Join button, with the reason once a join failed.
//!   The window joins by itself only for a stream you asked to watch
//!   ([`StreamJoinIntent`]).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::calls::stream_title::{SEPARATOR_GAP, title_layout, title_rect};
use super::calls::stream_view::blurred_photo;
use super::calls::stream_volume::{KEY_STEP, VolumeHost};
use super::group_call_panel::LEAVE_BG;
use super::group_call_stream_boxes::StreamOverlay;
use super::video_player::VideoPlayer;
use super::video_player::controls::{
    BADGE_GAP, BAR_HEIGHT, CONTROLS_BG, ControlsFade, LiveActions, LiveFacts, LiveKeyAction,
    VideoController, hide_cursor, live_key_action, live_label, live_pill, live_pill_text_size,
    live_pill_width, stream_label_box,
};
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_kit::component::slider::SliderState;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::calls::engine::StreamState;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with, trn_with};
use quill::state::ActiveGroupCall;
use quill::telegram::envelope::MessageSender;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

/// `groupCallNarrowSkip`, `groupCallWideVideoTop`.
const SKIP: f32 = 9.;
const VIDEO_TOP: f32 = 24.;
/// `groupCallButtonBottomSkipWide`.
const BAR_BOTTOM: f32 = 108.;
/// `groupCallTitleLabel` (semibold 14 px) and the backdrop's radius.
const TITLE_SIZE: f32 = 14.;
const TITLE_RADIUS: f32 = 6.;
/// The pin button's width with its margin (`groupCallPinOnTop`).
const PIN_WIDTH: f32 = 40.;
/// How far a press on the title band moves before it drags the window,
/// so a double-click still toggles full screen.
const DRAG_THRESHOLD: f32 = 4.;
/// A join that neither lands nor reports an error gives up after this.
const JOIN_TIMEOUT: Duration = Duration::from_secs(20);

/// You asked to watch a stream, or created a call: the window joins it
/// once it is tracked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StreamJoinIntent {
    /// The chat's video chat button or a stream link: this call, joined
    /// when it is a stream.
    Call(i32),
    /// You created the chat's new call (tdesktop `createGroupCall` joins
    /// it right away): joined whatever its kind, as `join_as` when set.
    Created {
        chat: i64,
        join_as: Option<MessageSender>,
    },
}

impl StreamJoinIntent {
    fn matches(self, call: i32, chat: Option<i64>) -> bool {
        match self {
            Self::Call(id) => id == call,
            Self::Created { chat: id, .. } => chat == Some(id),
        }
    }
}

/// The stream window's state, kept between renders.
#[derive(Default)]
pub(crate) struct StreamWindow {
    /// The stream's live player, over the driver's feed for the chat.
    pub(super) player: Option<VideoPlayer>,
    /// The blurred chat photo for a photo path (`None` while it blurs).
    pub(super) photo: Option<(String, Option<Arc<RenderImage>>)>,
    photo_task: Option<Task<()>>,
    pub(super) controls: ControlsFade,
    hide_task: Option<Task<()>>,
    hide_armed: bool,
    /// The open window was sized for this layout.
    pub(super) window_is_stream: bool,
    /// The stream you asked to watch, until it is tracked, with the call
    /// tracked when you asked (another call taking its place drops it).
    pub(super) pending_join: Option<(StreamJoinIntent, Option<i32>)>,
    /// The call whose join was sent (the Join button waits).
    pub(super) joining: Option<i32>,
    /// Gives up on a join that neither lands nor fails.
    join_timeout: Option<Task<()>>,
    pub(super) overlay: Option<StreamOverlay>,
    /// The Stream Key was revealed after its warning.
    pub(super) key_shown: bool,
    /// Where the title band was pressed: a move past a few pixels drags
    /// the window (Linux).
    title_pressed: Option<Point<Pixels>>,
    /// Screenshot fixtures and tests: the stream's feed without a driver.
    pub(super) demo_feed: Option<super::calls::demo_feed::DemoFeed>,
    /// Screenshot fixtures and tests: "mute" without a call engine.
    pub(super) demo_muted: bool,
    pub(super) volume: Option<Entity<SliderState>>,
}

impl StreamWindow {
    /// The call ended or another took its place: drop the player and the
    /// photo from every window and forget what the window showed.
    pub(super) fn clear(&mut self, cx: &mut App) {
        if let Some(image) = self.photo.take().and_then(|(_, image)| image) {
            cx.drop_image(image, None);
        }
        if let Some(player) = self.player.take() {
            player.release_live(cx);
        }
        self.photo_task = None;
        self.demo_feed = None;
        self.overlay = None;
        self.key_shown = false;
        self.joining = None;
        self.join_timeout = None;
        self.volume = None;
    }
}

/// What the stream window shows, read once per draw without copying the
/// call's participants or messages.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StreamFacts {
    pub(crate) id: i32,
    /// The chat's id and name.
    pub(crate) chat: Option<(i64, String)>,
    /// The call's title, or the chat's name.
    pub(crate) title: String,
    pub(crate) stream: StreamState,
    pub(crate) joined: bool,
    pub(crate) manager: bool,
    pub(crate) viewers: u32,
    /// The call knows its RTMP server URL (the admin's empty label).
    pub(crate) server_known: bool,
    pub(crate) recording: bool,
    pub(crate) share_link: bool,
    /// The native connect reported an error.
    pub(crate) transport_error: bool,
}

/// Whether `call` shows in the live stream layout: a running RTMP stream
/// (tdesktop `_call->rtmp()`). Live stories have their own viewer, and a
/// scheduled stream keeps the scheduled panel.
pub(super) fn uses_stream_layout(call: &ActiveGroupCall) -> bool {
    call.is_rtmp_stream && !call.is_live_story && call.scheduled_start_date == 0
}

/// The stream window draws full screen: full screen or maximized
/// (tdesktop treats both alike), or a capture asked for it
/// (`QUILL_DEMO_FULL=1`; a capture cannot go full screen).
pub(crate) fn stream_full(window: &Window, app: &QuillApp) -> bool {
    window.is_fullscreen() || window.is_maximized() || app.demo_ui.group_stage
}

/// Full screen or maximized off (tdesktop `toggleFullScreen(false)`:
/// `showNormal`).
pub(super) fn leave_stream_full(window: &mut Window) {
    if window.is_fullscreen() {
        window.toggle_fullscreen();
    } else if window.is_maximized() {
        window.zoom_window();
    }
}

/// The round button and a title double-click (`toggleFullScreen()`):
/// back to normal when full screen or maximized, else full screen.
pub(super) fn toggle_stream_full(window: &mut Window) {
    if window.is_fullscreen() || window.is_maximized() {
        leave_stream_full(window);
    } else if !quill::agent::ui_driver::guard::refuse_hard_if_injecting(
        quill::agent::ui_driver::guard::HardSurface::Window,
    ) {
        window.toggle_fullscreen();
    }
}

/// The picture's inset from the window's sides and top
/// (`groupCallNarrowSkip`, `groupCallWideVideoTop`); none full screen.
pub(super) fn stream_viewport(full: bool) -> (f32, f32) {
    if full { (0., 0.) } else { (SKIP, VIDEO_TOP) }
}

/// What the stream body needs from the window after it is built.
pub(super) struct StreamBody {
    pub(super) element: AnyElement,
    /// The controls slide: draw the next frame too.
    pub(super) animating: bool,
}

impl QuillApp {
    /// The tracked call's facts when it shows as a live stream.
    pub(super) fn group_call_stream_facts(&self) -> Option<StreamFacts> {
        let call = self.session()?.calls.active_group_call.as_ref()?;
        if !uses_stream_layout(call) {
            return None;
        }
        let chat = self
            .group_call_chat(call)
            .map(|chat| (chat.id.0, chat.title.clone()));
        let title = if call.title.is_empty() {
            chat.as_ref()
                .map(|(_, name)| name.clone())
                .unwrap_or_default()
        } else {
            call.title.clone()
        };
        Some(StreamFacts {
            id: call.id,
            chat,
            title,
            stream: call.stream,
            joined: call.is_joined,
            manager: call.can_be_managed,
            viewers: call.viewer_count,
            server_known: call.rtmp_url.as_deref().is_some_and(|url| !url.is_empty()),
            recording: call.record_duration > 0,
            share_link: self.group_call_can_share_link(call),
            transport_error: call.transport_error.is_some(),
        })
    }

    /// You asked to watch a stream (or a call that may turn out to be
    /// one): the window joins it once it is tracked.
    pub(super) fn note_stream_join_intent(&mut self, intent: StreamJoinIntent) {
        let tracked = self
            .session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            .map(|call| call.id);
        self.group_call.stream.pending_join = Some((intent, tracked));
        // Asking again brings a window you closed back.
        self.group_call.window_closed_by_user = None;
    }

    /// Join the tracked stream from its window: the Join button, or the
    /// join you asked for.
    pub(super) fn join_stream(&mut self, id: i32, cx: &mut Context<Self>) {
        self.dismiss_group_call_error(cx);
        if self.send_group_call_join() {
            self.group_call.stream.joining = Some(id);
            self.group_call.stream.join_timeout = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(JOIN_TIMEOUT).await;
                let _ = this.update(cx, |app, cx| app.stream_join_ended(id, cx));
            }));
        } else {
            self.stream_join_failed();
        }
        cx.notify();
    }

    /// The join was not sent or did not land: stop waiting and say so.
    fn stream_join_failed(&mut self) {
        self.group_call.stream.joining = None;
        self.group_call.stream.join_timeout = None;
        let reason = tr(keys::Q_OPS_STREAM_JOIN_FAILED).to_string();
        if let Some(live) = self.live.as_mut() {
            live.driver.session.calls.group_call_error = Some(reason);
        } else if let Some(session) = self.demo_session.as_mut() {
            session.calls.group_call_error = Some(reason);
        }
    }

    /// The join of `id` timed out, or the window saw it end: unless it
    /// landed, the Join button comes back with the failure.
    pub(super) fn stream_join_ended(&mut self, id: i32, cx: &mut Context<Self>) {
        if self.group_call.stream.joining != Some(id) {
            return;
        }
        let joined = self
            .session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            .is_some_and(|call| call.id == id && call.is_joined);
        if joined {
            self.group_call.stream.joining = None;
            self.group_call.stream.join_timeout = None;
        } else {
            self.stream_join_failed();
        }
        cx.notify();
    }

    /// Run the join you asked for once its call is tracked. Any other
    /// unjoined stream waits for its Join button.
    pub(super) fn consume_stream_join_intent(&mut self, cx: &mut Context<Self>) {
        let Some((intent, tracked_then)) = self.group_call.stream.pending_join else {
            return;
        };
        let Some(call) = self
            .session()
            .and_then(|s| s.calls.active_group_call.as_ref())
        else {
            return;
        };
        let chat = self.group_call_chat(call).map(|chat| chat.id.0);
        if !intent.matches(call.id, chat) {
            // Another call took the place of the one tracked when you
            // asked: the wish is stale and must not join much later.
            if tracked_then != Some(call.id) {
                self.group_call.stream.pending_join = None;
            }
            return;
        }
        let (id, stream, joined) = (call.id, uses_stream_layout(call), call.is_joined);
        self.group_call.stream.pending_join = None;
        if joined {
            return;
        }
        match intent {
            StreamJoinIntent::Call(_) if stream => self.join_stream(id, cx),
            StreamJoinIntent::Call(_) => {}
            StreamJoinIntent::Created { join_as, .. } => {
                if join_as.is_some() {
                    if let Some(live) = self.live.as_mut() {
                        live.driver.session.set_group_call_join_as(join_as);
                    } else if let Some(session) = self.demo_session.as_mut() {
                        session.set_group_call_join_as(join_as);
                    }
                }
                if stream {
                    self.join_stream(id, cx);
                } else {
                    self.send_group_call_join();
                    cx.notify();
                }
            }
        }
    }

    /// The window's content for a live stream. `full`: full screen or
    /// maximized.
    pub(super) fn group_call_stream_body(
        &mut self,
        facts: &StreamFacts,
        full: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> StreamBody {
        let now = cx.background_executor().now();
        self.group_call.stream.controls.set_full(full, now);
        self.group_call.stream.controls.tick(now);
        self.arm_stream_hide_timer(cx);
        let shown = self.group_call.stream.controls.value(now);
        let animating = self.group_call.stream.controls.sliding(now);
        match self.group_call.stream.joining {
            Some(id) if id != facts.id => self.stream_join_failed(),
            Some(_) if facts.joined => {
                self.group_call.stream.joining = None;
                self.group_call.stream.join_timeout = None;
            }
            // The connect failed without a join error: stop waiting.
            Some(_) if facts.transport_error => self.stream_join_failed(),
            Some(_)
                if self
                    .session()
                    .is_some_and(|s| s.calls.group_call_error.is_some()) =>
            {
                self.group_call.stream.joining = None;
                self.group_call.stream.join_timeout = None;
            }
            _ => {}
        }

        let chat_id = facts.chat.as_ref().map(|(id, _)| *id);
        let chat_name = facts
            .chat
            .as_ref()
            .map(|(_, name)| name.as_str())
            .unwrap_or_default();
        // The picture view shows the photo once it is blurred.
        if let Some(id) = chat_id {
            self.group_call_stream_photo(id, window, cx);
        }
        let (inset, top) = stream_viewport(full);
        let viewport = div()
            .id("group-call-stream-viewport")
            .debug_selector(|| "group-call-stream-viewport".into())
            .absolute()
            .left(px(inset))
            .right(px(inset))
            .bottom(px(inset))
            .top(px(top))
            .on_hover(cx.listener(|this, inside: &bool, _, cx| {
                this.group_call
                    .stream
                    .controls
                    .hover_picture(*inside, cx.background_executor().now());
                cx.notify();
            }))
            .on_mouse_move(cx.listener(|this, _: &MouseMoveEvent, _, cx| {
                if this.group_call.stream.controls.is_full() {
                    this.group_call
                        .stream
                        .controls
                        .mouse_moved(cx.background_executor().now());
                    this.arm_stream_hide_timer(cx);
                    cx.notify();
                }
            }));

        let width = f32::from(window.viewport_size().width);
        let label = if facts.joined {
            stream_label(facts, chat_name).map(|text| stream_label_box(text, width, None))
        } else {
            Some(self.stream_join_label(facts, width, cx))
        };
        let viewers = i64::from(facts.viewers.max(1));
        let viewers = trn_with(
            keys::GROUP_CALL_RTMP_VIEWERS,
            viewers,
            &[("count", &quill::premium_hub::group_digits(viewers))],
        );
        let title = title_row(&facts.title, &viewers, full, shown, window);
        let volume = self.stream_volume_slider(VolumeHost::StreamWindow, Some(facts.id), cx);
        let bar = self
            .group_call_stream_bar(facts, volume, full, shown, cx)
            .into_any_element();
        let band = (!full).then(|| self.stream_title_band(cx));
        let share = if self.group_call.share_open {
            self.session()
                .and_then(|s| s.calls.active_group_call.as_ref())
                .and_then(|call| self.group_call_share_overlay(call, cx))
        } else {
            None
        };
        let rename =
            self.group_call_rename_overlay(tr(keys::GROUP_CALL_EDIT_TITLE_CHANNEL).into(), cx);
        let overlay = self.group_call_stream_overlay(facts, cx);

        // No background: the window's root paints it under the picture.
        let element = div()
            .id("group-call-stream")
            .size_full()
            .relative()
            .overflow_hidden()
            .text_color(white())
            .child(viewport)
            .children(band)
            .children(label)
            .child(title)
            .child(bar)
            .children(share)
            .children(rename)
            .children(overlay)
            .into_any_element();
        StreamBody { element, animating }
    }

    /// The 24 px band over the picture moves the window, and a
    /// double-click toggles full screen (the window's maximize request,
    /// which an RTMP panel turns into full screen). Windows treats it as
    /// the caption; Linux starts the move from a press and a move, as the
    /// kit title bar does; macOS drags its transparent title bar.
    fn stream_title_band(&self, cx: &mut Context<Self>) -> AnyElement {
        let band = div()
            .id("group-call-stream-title-band")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(VIDEO_TOP))
            .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                this.group_call.stream.title_pressed = None;
                if event.click_count() == 2 {
                    toggle_stream_full(window);
                    cx.notify();
                }
            }));
        let band = if cfg!(target_os = "windows") {
            band.window_control_area(WindowControlArea::Drag)
        } else {
            band
        };
        if !cfg!(target_os = "linux") {
            return band.into_any_element();
        }
        band.on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, event: &MouseDownEvent, _, _| {
                this.group_call.stream.title_pressed = Some(event.position)
            }),
        )
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|this, _, _, _| this.group_call.stream.title_pressed = None),
        )
        .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, _| {
            let stream = &mut this.group_call.stream;
            if event.pressed_button != Some(MouseButton::Left) {
                stream.title_pressed = None;
                return;
            }
            let Some(from) = stream.title_pressed else {
                return;
            };
            let moved = event.position - from;
            if f32::from(moved.x).hypot(f32::from(moved.y)) >= DRAG_THRESHOLD {
                stream.title_pressed = None;
                window.start_window_move();
            }
        }))
        .into_any_element()
    }

    /// The bottom bar: the live controller with the menu before it and
    /// Leave after it. It stays in the tree while hidden (slid below the
    /// window), so leaving it is always seen and no hover goes stale.
    fn group_call_stream_bar(
        &self,
        facts: &StreamFacts,
        volume: Option<Entity<SliderState>>,
        full: bool,
        shown: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let slot = |width: f32| {
            div()
                .w(px(width))
                .h(px(BAR_HEIGHT))
                .flex()
                .items_center()
                .justify_center()
        };
        let leave_facts = facts.clone();
        let leave = slot(60.).child(
            Button::new("group-call-stream-leave")
                .custom(
                    ButtonCustomVariant::new(cx)
                        .color(rgba(LEAVE_BG).into())
                        .hover(rgba(0xf75c5cb3).into())
                        .active(rgba(0xf75c5cd9).into())
                        .foreground(white()),
                )
                .icon(Icon::new(IconName::Close).with_size(px(22.)))
                .rounded(px(22.))
                .size(px(44.))
                .tooltip(tr(keys::GROUP_CALL_LEAVE))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.stream_leave_clicked(&leave_facts, window, cx)
                })),
        );
        let view = cx.entity().downgrade();
        let mute_view = view.clone();
        let full_view = view.clone();
        let pip_view = view.clone();
        let actions = LiveActions {
            toggle_mute: Rc::new(move |_, cx| {
                let _ = mute_view.update(cx, |this, cx| this.toggle_stream_mute(cx));
            }),
            fullscreen: Some(Rc::new(move |window, cx| {
                toggle_stream_full(window);
                let _ = full_view.update(cx, |_, cx| cx.notify());
            })),
            // The live PiP (`calls::live_pip`); the window's own pin on
            // top stays where it is.
            pip: Some(Rc::new(move |window, cx| {
                let _ = pip_view.update(cx, |this, cx| this.open_stream_pip(window, cx));
            })),
            volume_hover: None,
            hover: Some(Rc::new(move |inside, window, cx| {
                let active = window.is_window_active();
                let _ = view.update(cx, |this, cx| {
                    let controls = &mut this.group_call.stream.controls;
                    let was = controls.shown();
                    controls.hover_controls(inside, cx.background_executor().now());
                    // Leaving the controls in full screen hides them at
                    // once: the cursor goes with them, as on the timer.
                    // Checked once the event settles: a move over the
                    // picture in the same event shows them again
                    // (tdesktop's Leave, then the viewport's MouseMove),
                    // and the cursor must not flash off.
                    if was && !controls.shown() {
                        let app = cx.entity().downgrade();
                        cx.defer(move |cx| {
                            let _ = app.update(cx, |this, _| this.hide_stream_cursor(active));
                        });
                    }
                    this.arm_stream_hide_timer(cx);
                    cx.notify();
                });
            })),
        };
        let live = LiveFacts {
            muted: self.stream_muted(),
            volume,
            volume_open: false,
            volume_focus: None,
            fullscreen: full,
        };
        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom(px(BAR_BOTTOM * shown - BAR_HEIGHT))
            .flex()
            .justify_center()
            .child(
                VideoController::live(u64::from(facts.id.unsigned_abs()), live, actions)
                    .leading(slot(60.).child(self.group_call_stream_menu(facts, cx)))
                    .trailing(leave),
            )
    }

    /// Whether you muted the stream's sound for yourself (tdesktop's
    /// local mute, the Settings box's "Mute for me").
    pub(super) fn stream_muted(&self) -> bool {
        match self.live.as_ref() {
            Some(live) => live
                .driver
                .session
                .calls
                .active_group_call
                .as_ref()
                .is_some_and(|call| live.driver.group_call_playback_muted(call.id)),
            None => self.group_call.stream.demo_muted,
        }
    }

    /// The bar's mute button and the M key: the stream's sound off or on
    /// for you only, through the call's playback.
    pub(super) fn toggle_stream_mute(&mut self, cx: &mut Context<Self>) {
        let muted = self.stream_muted();
        match self.live.as_mut() {
            Some(live) => {
                let call_id = live
                    .driver
                    .session
                    .calls
                    .active_group_call
                    .as_ref()
                    .map(|c| c.id);
                if let Some(call_id) = call_id
                    && let Err(err) = live.driver.set_group_call_playback_muted(call_id, !muted)
                {
                    self.connection.set_note(StatusNote::error(err.to_string()));
                }
            }
            None => self.group_call.stream.demo_muted = !muted,
        }
        cx.notify();
    }

    /// A key in the stream window (`live_key_action`): `true` when the
    /// live controller took it. Not while a box or field is open over the
    /// stream, where the keys are typing.
    pub(super) fn stream_key_down(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let stream = &self.group_call.stream;
        if stream.overlay.is_some()
            || self.group_call.share_open
            || self.group_call.title_dialog.is_some()
        {
            return false;
        }
        let full = stream_full(window, self);
        let Some(action) = live_key_action(&keystroke.key, &keystroke.modifiers, full) else {
            return false;
        };
        match action {
            LiveKeyAction::Nothing => {}
            LiveKeyAction::ToggleMute => self.toggle_stream_mute(cx),
            LiveKeyAction::VolumeUp | LiveKeyAction::VolumeDown => {
                let step = if action == LiveKeyAction::VolumeUp {
                    KEY_STEP
                } else {
                    -KEY_STEP
                };
                let call_id = self
                    .session()
                    .and_then(|s| s.calls.active_group_call.as_ref())
                    .map(|c| c.id);
                self.step_stream_volume(step, call_id, cx);
            }
            LiveKeyAction::ToggleFullscreen => toggle_stream_full(window),
            LiveKeyAction::LeaveFullscreen => leave_stream_full(window),
        }
        cx.notify();
        true
    }

    /// Not joined: the label with a Join button, and the reason when a
    /// join failed. The button spins while the join is on its way.
    fn stream_join_label(
        &self,
        facts: &StreamFacts,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let error = self
            .session()
            .and_then(|s| s.calls.group_call_error.clone());
        let joining = self.group_call.stream.joining == Some(facts.id);
        let id = facts.id;
        let button = Button::new("group-call-stream-join")
            .label(tr(keys::GROUP_CALL_JOIN))
            .primary()
            .loading(joining)
            .on_click(cx.listener(move |this, _, _, cx| this.join_stream(id, cx)));
        stream_label_box(
            error.unwrap_or_default(),
            width,
            Some(button.into_any_element()),
        )
    }

    /// The chat's photo, blurred once per photo off the UI thread.
    fn group_call_stream_photo(
        &mut self,
        chat_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Arc<RenderImage>> {
        let path = self
            .session()?
            .chat_photo_path(ChatId(chat_id))?
            .to_string();
        if let Some((cached, image)) = self.group_call.stream.photo.as_ref()
            && *cached == path
        {
            return image.clone();
        }
        if let Some((_, Some(old))) = self.group_call.stream.photo.take() {
            cx.drop_image(old, Some(window));
        }
        self.group_call.stream.photo = Some((path.clone(), None));
        let task = cx.spawn(async move |this, cx| {
            let source = path.clone();
            let image = cx
                .background_spawn(async move { blurred_photo(Path::new(&source)) })
                .await;
            let _ = this.update(cx, |app, cx| {
                if let Some((cached, slot)) = app.group_call.stream.photo.as_mut()
                    && *cached == path
                {
                    *slot = image;
                    cx.notify();
                }
            });
        });
        self.group_call.stream.photo_task = Some(task);
        None
    }

    /// Full screen: hide the controls when the 5 s timer runs out. One
    /// task runs while the timer does; a move only pushes its deadline.
    /// The controls just hid: in full screen the cursor goes too (tdesktop
    /// `setCursorShown(!_rtmpFull || shown)`). The platform call hides it
    /// system wide, so only while the pointer is over this window's picture
    /// or controls and the window is active (tdesktop sets a blank cursor
    /// on its own widget).
    fn hide_stream_cursor(&self, window_active: bool) {
        let controls = &self.group_call.stream.controls;
        if window_active && controls.cursor_hidden() && controls.pointer_inside() {
            hide_cursor();
        }
    }

    fn arm_stream_hide_timer(&mut self, cx: &mut Context<Self>) {
        let stream = &mut self.group_call.stream;
        if stream.hide_armed || stream.controls.hide_at().is_none() {
            return;
        }
        stream.hide_armed = true;
        stream.hide_task = Some(cx.spawn(async move |this, cx| {
            loop {
                let next = this.update(cx, |app, _| app.group_call.stream.controls.hide_at());
                let Ok(Some(at)) = next else {
                    break;
                };
                let wait = at.saturating_duration_since(cx.background_executor().now());
                cx.background_executor().timer(wait).await;
                // Asked outside the app's update: the window's own state.
                let handle = this
                    .update(cx, |app, _| app.group_call.window)
                    .ok()
                    .flatten();
                let active = handle
                    .and_then(|handle| {
                        cx.update_window(handle, |_, window, _| window.is_window_active())
                            .ok()
                    })
                    .unwrap_or(false);
                let more = this.update(cx, |app, cx| {
                    let controls = &mut app.group_call.stream.controls;
                    let was = controls.shown();
                    controls.tick(cx.background_executor().now());
                    if was != controls.shown() {
                        app.hide_stream_cursor(active);
                        cx.notify();
                    }
                    app.group_call.stream.controls.hide_at().is_some()
                });
                if !matches!(more, Ok(true)) {
                    break;
                }
            }
            let _ = this.update(cx, |app, _| app.group_call.stream.hide_armed = false);
        }));
    }
}

/// The label over the picture: nobody is broadcasting (the admin text
/// when the call knows its RTMP server URL, as tdesktop's
/// `rtmpInfo().url`).
fn stream_label(facts: &StreamFacts, chat_name: &str) -> Option<String> {
    match facts.stream {
        StreamState::NoStream if facts.server_known => {
            Some(tr(keys::GROUP_CALL_NO_STREAM_ADMIN).to_string())
        }
        StreamState::NoStream => Some(tr_with(keys::GROUP_CALL_NO_STREAM, &[("group", chat_name)])),
        StreamState::Connecting | StreamState::Live | StreamState::Ended => None,
    }
}

/// The natural width of `text` in the title's font.
fn title_text_width(text: &str, window: &mut Window) -> f32 {
    text_width(text, px(TITLE_SIZE), window)
}

/// The natural width of `text`, semibold at `size`.
fn text_width(text: &str, size: Pixels, window: &mut Window) -> f32 {
    let mut font = window.text_style().font();
    font.weight = FontWeight::SEMIBOLD;
    let run = TextRun {
        len: text.len(),
        font,
        color: white(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line =
        window
            .text_system()
            .shape_line(SharedString::from(text.to_string()), size, &[run], None);
    f32::from(line.width).ceil()
}

/// The title, a bullet, the LIVE pill and the viewer count
/// (`refreshTitleGeometry`; the pill is the live controller's badge).
/// Full screen it slides up with the controls on a backdrop rounded at
/// the bottom (`refreshTitleBackground`).
fn title_row(
    title: &str,
    viewers: &str,
    full: bool,
    shown: f32,
    window: &mut Window,
) -> AnyElement {
    let width = f32::from(window.viewport_size().width);
    let title_width = title_text_width(title, window);
    let bullet_width = title_text_width("\u{2022}", window);
    // The pill and its gap travel with the count and never shrink.
    let pill =
        live_pill_width(text_width(&live_label(), live_pill_text_size(), window)) + BADGE_GAP;
    let viewers_width = title_text_width(viewers, window) + pill;
    // Full screen the pin button is gone and its room goes to the line.
    let pin = if full { 0. } else { PIN_WIDTH };
    let rect = title_rect(width, pin, cfg!(target_os = "macos") && !full);
    let layout = title_layout(width, rect, title_width, bullet_width, viewers_width);
    // The text sits 3 px down the 24 px band; full screen the backdrop
    // reaches the top edge and slides out by its own height.
    let offset = if full { -25. * (1. - shown) } else { 0. };
    div()
        .id("group-call-stream-title")
        .debug_selector(|| "group-call-stream-title".into())
        .absolute()
        .top(px(offset))
        .left(px(layout.left - TITLE_RADIUS))
        .pt(px(3.))
        .pb(px(4.))
        .px(px(TITLE_RADIUS))
        .when(full, |this| {
            this.rounded_b(px(TITLE_RADIUS)).bg(rgba(CONTROLS_BG))
        })
        .flex()
        .items_center()
        .gap(px(SEPARATOR_GAP))
        .text_size(px(TITLE_SIZE))
        .font_weight(FontWeight::SEMIBOLD)
        .child(
            div()
                .w(px(layout.title))
                .truncate()
                .child(title.to_string()),
        )
        .child(div().flex_none().child("\u{2022}"))
        .child(
            div()
                .w(px(layout.viewers))
                .flex()
                .items_center()
                .gap(px(BADGE_GAP))
                .child(live_pill())
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .truncate()
                        .child(viewers.to_string()),
                ),
        )
        .into_any_element()
}

#[cfg(feature = "demo-capture")]
#[path = "group_call_stream_demo.rs"]
pub(super) mod demo;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "group_call_stream_tests.rs"]
mod ui_tests;
