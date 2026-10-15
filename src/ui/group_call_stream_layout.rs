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
//! - Title row: the call's title (or the chat's name), a bullet and
//!   "{count} viewers" (`lng_group_call_rtmp_viewers`, at least 1), placed
//!   by [`title_layout`]. Full screen it slides with the controls on a
//!   rounded backdrop.
//! - Bottom bar (`updateButtonsGeometry`, wide RTMP branch): the "…" menu
//!   for managers or Settings for viewers, the round button that toggles
//!   full screen, Leave, on a rounded `groupCallBg` backdrop at 80 %.
//! - Before the first picture the tile shows the chat's photo blurred
//!   (radius 15). Nobody broadcasting: a rounded label a third of the way
//!   down (`setupEmptyRtmp`), with the admin text when the call knows its
//!   RTMP server URL.
//! - When the controls show: see [`StreamControls`].
//! - Not joined (Quill only: tdesktop never shows the panel unjoined):
//!   the label holds a Join button, with the reason once a join failed.
//!   The window joins by itself only for a stream you asked to watch
//!   ([`StreamJoinIntent`]).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::calls::stream_controls::StreamControls;
use super::calls::stream_picture::{StreamPicture, frame_image};
use super::calls::stream_tile::{StreamTile, StreamTileState, WidthReport, blurred_photo};
use super::calls::stream_title::{SEPARATOR_GAP, title_layout, title_rect};
use super::group_call_panel::{BG, LEAVE_BG, MEMBER_NOT_JOINED, MEMBERS_BG};
use super::group_call_stream_boxes::StreamOverlay;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::calls::engine::{StreamState, VideoFrame};
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with, trn_with};
use quill::state::ActiveGroupCall;
use quill::telegram::envelope::MessageSender;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// `groupCallMuted1` / `groupCallMuted2`: the round button's gradient.
const BUTTON_FROM: u32 = 0x0992ef;
const BUTTON_TO: u32 = 0x16ccfb;
/// `groupCallBg` at `kControlsBackgroundOpacity` (0.8).
const CONTROLS_BG: u32 = 0x1a2026cc;
/// `groupCallNarrowSkip`, `groupCallWideVideoTop`.
const SKIP: f32 = 9.;
const VIDEO_TOP: f32 = 24.;
/// `groupCallButtonBottomSkipWide`; the buttons are 68 px tall.
const BAR_BOTTOM: f32 = 108.;
const BAR_HEIGHT: f32 = 68.;
/// `st::groupCallWidth / 30` and `st::groupCallWidth`: the label's
/// padding and widest text; the text keeps 24 px from each side.
const LABEL_PADDING: f32 = 12.;
const LABEL_WIDTH: f32 = 380.;
const LABEL_SIDES: f32 = 48.;
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
    pub(super) tile: StreamTileState,
    pub(super) picture: StreamPicture,
    /// The blurred chat photo for a photo path (`None` while it blurs).
    photo: Option<(String, Option<Arc<RenderImage>>)>,
    photo_task: Option<Task<()>>,
    pub(super) controls: StreamControls,
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
    /// Screenshot fixtures and tests: the stream picture without a driver.
    pub(super) demo_frame: Option<Arc<VideoFrame>>,
}

impl StreamWindow {
    /// The call ended or another took its place: drop the pictures from
    /// every window and forget what the window showed.
    pub(super) fn clear(&mut self, cx: &mut App) {
        let images = self
            .picture
            .clear()
            .into_iter()
            .chain(self.photo.take().and_then(|(_, image)| image));
        for image in images {
            cx.drop_image(image, None);
        }
        self.photo_task = None;
        self.demo_frame = None;
        self.overlay = None;
        self.key_shown = false;
        self.joining = None;
        self.join_timeout = None;
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
    } else {
        window.toggle_fullscreen();
    }
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
        let now = Instant::now();
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
        let picture = chat_id.and_then(|id| self.group_call_stream_picture(id, window, cx));
        let placeholder = chat_id.and_then(|id| self.group_call_stream_photo(id, window, cx));
        // Redraws at the stream's rate only while pictures come.
        let playing = facts.joined && facts.stream == StreamState::Live;

        let owner = cx.entity().downgrade();
        let report: Option<WidthReport> = chat_id.map(|chat_id| {
            let owner = owner.clone();
            let report: WidthReport = Rc::new(move |width, cx: &mut App| {
                let _ = owner.update(cx, |app, _| {
                    if let Some(live) = app.live.as_mut() {
                        live.driver.report_stream_tile_width(chat_id, width);
                    }
                });
            });
            report
        });
        let tile = StreamTile::new("group-call-stream-tile", &self.group_call.stream.tile)
            .picture(picture)
            .placeholder(placeholder)
            .fit(ObjectFit::Contain)
            .frame(
                px(if full { 0. } else { 6. }),
                if full {
                    black()
                } else {
                    rgb(MEMBERS_BG).into()
                },
            )
            .playing(playing)
            .when_some(report, |tile, report| tile.on_width(report));
        let inset = if full { 0. } else { SKIP };
        let viewport = div()
            .id("group-call-stream-viewport")
            .debug_selector(|| "group-call-stream-viewport".into())
            .absolute()
            .left(px(inset))
            .right(px(inset))
            .bottom(px(inset))
            .top(px(if full { 0. } else { VIDEO_TOP }))
            .on_hover(cx.listener(|this, inside: &bool, _, cx| {
                this.group_call
                    .stream
                    .controls
                    .hover_picture(*inside, Instant::now());
                cx.notify();
            }))
            .on_mouse_move(cx.listener(|this, _: &MouseMoveEvent, _, cx| {
                if this.group_call.stream.controls.is_full() {
                    this.group_call.stream.controls.mouse_moved(Instant::now());
                    this.arm_stream_hide_timer(cx);
                    cx.notify();
                }
            }))
            .child(tile);

        let width = f32::from(window.viewport_size().width);
        let label = if facts.joined {
            stream_label(facts, chat_name).map(|text| label_view(text, width, None))
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
        let bar = self
            .group_call_stream_bar(facts, full, shown, cx)
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

        let element = div()
            .id("group-call-stream")
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(if full { black() } else { rgb(BG).into() })
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

    /// The bottom bar. It stays in the tree while hidden (slid below the
    /// window), so leaving it is always seen and no hover goes stale.
    fn group_call_stream_bar(
        &self,
        facts: &StreamFacts,
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
        // A gradient disc: the kit's custom button takes one solid color.
        let expand = slot(68.).child(
            div()
                .id("group-call-stream-fullscreen")
                .size(px(42.))
                .rounded_full()
                .bg(linear_gradient(
                    135.,
                    linear_color_stop(rgb(BUTTON_FROM), 0.),
                    linear_color_stop(rgb(BUTTON_TO), 1.),
                ))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .role(Role::Button)
                .aria_label(SharedString::from(tr(keys::Q_MISC_TOGGLE_FULL_SCREEN)))
                .hover(|style| style.opacity(0.85))
                .on_click(cx.listener(|_, _, window, cx| {
                    toggle_stream_full(window);
                    cx.notify();
                }))
                .child(
                    Icon::new(if full {
                        IconName::Minimize
                    } else {
                        IconName::Maximize
                    })
                    .with_size(px(20.))
                    .text_color(white()),
                ),
        );
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
        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom(px(BAR_BOTTOM * shown - BAR_HEIGHT))
            .flex()
            .justify_center()
            .child(
                div()
                    .id("group-call-stream-bar")
                    .debug_selector(|| "group-call-stream-bar".into())
                    .h(px(BAR_HEIGHT))
                    .px(px(10.))
                    .rounded(px(12.))
                    .bg(rgba(CONTROLS_BG))
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .on_hover(cx.listener(|this, inside: &bool, _, cx| {
                        this.group_call
                            .stream
                            .controls
                            .hover_controls(*inside, Instant::now());
                        this.arm_stream_hide_timer(cx);
                        cx.notify();
                    }))
                    .child(slot(60.).child(self.group_call_stream_menu(facts, cx)))
                    .child(expand)
                    .child(leave),
            )
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
        label_view(
            error.unwrap_or_default(),
            width,
            Some(button.into_any_element()),
        )
    }

    /// The newest stream picture of `chat_id`. A new frame is converted
    /// off the UI thread, and the image from two frames back leaves every
    /// window's atlas at once ([`StreamPicture`]).
    fn group_call_stream_picture(
        &mut self,
        chat_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Arc<RenderImage>> {
        let seq = match self.live.as_ref() {
            Some(live) => live.driver.latest_stream_frame_seq(chat_id),
            None => self
                .group_call
                .stream
                .demo_frame
                .as_ref()
                .map(|frame| frame.seq),
        };
        if let Some(seq) = seq
            && self.group_call.stream.picture.wants(chat_id, seq)
        {
            let frame = match self.live.as_ref() {
                Some(live) => live.driver.latest_stream_frame(chat_id),
                None => self.group_call.stream.demo_frame.clone(),
            };
            // A screenshot fixture's still picture is converted at once so
            // the capture's first frame shows it; tests keep the
            // background path the live stream takes.
            let inline = self.live.is_none() && !cfg!(test);
            if let Some(frame) = frame.as_ref().filter(|_| inline) {
                let image = frame_image(frame);
                let picture = &mut self.group_call.stream.picture;
                picture.start(chat_id, frame.seq, Task::ready(()));
                // Inside this window's draw: name it, or its atlas keeps
                // the image.
                if let Some(old) = picture.finish(chat_id, frame.seq, image) {
                    cx.drop_image(old, Some(window));
                }
            } else if let Some(frame) = frame {
                let seq = frame.seq;
                let task = cx.spawn(async move |this, cx| {
                    let image = cx
                        .background_spawn(async move { frame_image(&frame) })
                        .await;
                    let _ = this.update(cx, |app, cx| {
                        let picture = &mut app.group_call.stream.picture;
                        if let Some(old) = picture.finish(chat_id, seq, image) {
                            cx.drop_image(old, None);
                        }
                        cx.notify();
                    });
                });
                self.group_call.stream.picture.start(chat_id, seq, task);
            }
        }
        self.group_call.stream.picture.current(chat_id)
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
                let wait = at.saturating_duration_since(Instant::now());
                cx.background_executor().timer(wait).await;
                let more = this.update(cx, |app, cx| {
                    let controls = &mut app.group_call.stream.controls;
                    let was = controls.shown();
                    controls.tick(Instant::now());
                    if was != controls.shown() {
                        cx.notify();
                    }
                    controls.hide_at().is_some()
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
/// `rtmpInfo().url`), or this build cannot play the stream.
fn stream_label(facts: &StreamFacts, chat_name: &str) -> Option<String> {
    match facts.stream {
        StreamState::NoStream if facts.server_known => {
            Some(tr(keys::GROUP_CALL_NO_STREAM_ADMIN).to_string())
        }
        StreamState::NoStream => Some(tr_with(keys::GROUP_CALL_NO_STREAM, &[("group", chat_name)])),
        StreamState::Unsupported => Some(tr(keys::Q_OPS_STREAM_UNSUPPORTED).to_string()),
        StreamState::Connecting | StreamState::Live | StreamState::Ended => None,
    }
}

/// tdesktop's empty-stream label (`setupEmptyRtmp`): semibold
/// `groupCallMemberNotJoinedStatus` text centered in a rounded box
/// `min(width - 48, 380) + 24` wide, a third of the way down. `action`
/// goes under the text.
fn label_view(text: String, window_width: f32, action: Option<AnyElement>) -> AnyElement {
    let inner = (window_width - LABEL_SIDES).clamp(0., LABEL_WIDTH);
    div()
        .absolute()
        .inset_0()
        .flex()
        .flex_col()
        .items_center()
        .child(div().flex_grow(1.))
        .child(
            div()
                .id("group-call-stream-label")
                .debug_selector(|| "group-call-stream-label".into())
                .w(px(inner + 2. * LABEL_PADDING))
                .p(px(LABEL_PADDING))
                .rounded(px(12.))
                .bg(rgba(CONTROLS_BG))
                .text_size(px(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(MEMBER_NOT_JOINED))
                .text_center()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(10.))
                .when(!text.is_empty(), |this| this.child(text))
                .children(action),
        )
        .child(div().flex_grow(2.))
        .into_any_element()
}

/// The natural width of `text` in the title's font.
fn title_text_width(text: &str, window: &mut Window) -> f32 {
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
    let line = window.text_system().shape_line(
        SharedString::from(text.to_string()),
        px(TITLE_SIZE),
        &[run],
        None,
    );
    f32::from(line.width).ceil()
}

/// The title, a bullet and the viewer count (`refreshTitleGeometry`).
/// Full screen it slides up with the controls on a backdrop rounded at
/// the bottom (`refreshTitleBackground`).
fn title_row(
    title: &str,
    viewers: &str,
    full: bool,
    shown: f32,
    window: &mut Window,
) -> impl IntoElement {
    let width = f32::from(window.viewport_size().width);
    let title_width = title_text_width(title, window);
    let bullet_width = title_text_width("\u{2022}", window);
    let viewers_width = title_text_width(viewers, window);
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
                .truncate()
                .child(viewers.to_string()),
        )
}

#[cfg(feature = "demo-capture")]
#[path = "group_call_stream_demo.rs"]
mod demo;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "group_call_stream_tests.rs"]
mod ui_tests;
