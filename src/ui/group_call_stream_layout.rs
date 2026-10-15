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
//!   "{count} viewers" (`lng_group_call_rtmp_viewers`, at least 1). Full
//!   screen it slides with the controls on a rounded backdrop.
//! - Bottom bar (`updateButtonsGeometry`, wide RTMP branch): the "…" menu
//!   for managers, the round button that toggles full screen, Leave, on a
//!   rounded `groupCallBg` backdrop at 80 %.
//! - Before the first picture the tile shows the chat's photo blurred
//!   (radius 15). Nobody broadcasting: a rounded label a third of the way
//!   down (`setupEmptyRtmp`), with the admin text when the call knows its
//!   RTMP server URL.
//! - When the controls show: see [`StreamControls`].
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::calls::stream_controls::StreamControls;
use super::calls::stream_tile::{StreamTile, StreamTileState, WidthReport, blurred_photo};
use super::group_call_panel::{BG, LEAVE_BG, MEMBER_NOT_JOINED, MEMBERS_BG};
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::calls::engine::{StreamState, VideoFrame};
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with, trn_with};
use quill::state::ActiveGroupCall;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

/// `groupCallMuted1` / `groupCallMuted2`: the round button's gradient.
const BUTTON_FROM: u32 = 0x0992ef;
const BUTTON_TO: u32 = 0x16ccfb;
/// `callIconBg`.
const ICON_BG: u32 = 0xffffff1f;
/// `groupCallBg` at `kControlsBackgroundOpacity` (0.8).
const CONTROLS_BG: u32 = 0x1a2026cc;
/// `groupCallNarrowSkip`, `groupCallWideVideoTop`.
const SKIP: f32 = 9.;
const VIDEO_TOP: f32 = 24.;
/// `groupCallButtonBottomSkipWide`; the buttons are 68 px tall.
const BAR_BOTTOM: f32 = 108.;
const BAR_HEIGHT: f32 = 68.;
/// `st::groupCallWidth / 30` and `st::groupCallWidth`: the label's
/// padding and widest text.
const LABEL_PADDING: f32 = 12.;
const LABEL_WIDTH: f32 = 380.;

/// The stream window's state, kept between renders.
#[derive(Default)]
pub(crate) struct StreamWindow {
    pub(super) tile: StreamTileState,
    /// The stream picture as an image, by chat and frame sequence.
    image: Option<(i64, u64, Arc<RenderImage>)>,
    /// The blurred chat photo for a photo path (`None` while it blurs).
    photo: Option<(String, Option<Arc<RenderImage>>)>,
    photo_task: Option<Task<()>>,
    pub(super) controls: StreamControls,
    hide_task: Option<Task<()>>,
    hide_armed: bool,
    /// The open window was sized for this layout.
    pub(super) window_is_stream: bool,
    /// The unjoined stream this window joined on opening.
    pub(super) auto_joined: Option<i32>,
    /// Screenshot fixtures: the stream picture without a live driver.
    pub(super) demo_frame: Option<VideoFrame>,
}

impl StreamWindow {
    /// Drop the pictures (the call ended or another took its place).
    pub(super) fn clear(&mut self) {
        let images = self
            .image
            .take()
            .map(|(_, _, image)| image)
            .into_iter()
            .chain(self.photo.take().and_then(|(_, image)| image));
        super::image_budget::retire_all(images);
        self.photo_task = None;
        self.demo_frame = None;
    }
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
    /// The window's content for a live stream. `full`: full screen or
    /// maximized.
    pub(super) fn group_call_stream_body(
        &mut self,
        call: &ActiveGroupCall,
        full: bool,
        cx: &mut Context<Self>,
    ) -> StreamBody {
        let now = Instant::now();
        self.group_call.stream.controls.set_full(full, now);
        self.group_call.stream.controls.tick(now);
        self.arm_stream_hide_timer(cx);
        let shown = self.group_call.stream.controls.value(now);
        let animating = self.group_call.stream.controls.sliding(now);

        let chat = self
            .group_call_chat(call)
            .map(|chat| (chat.id.0, chat.title.clone()));
        let chat_id = chat.as_ref().map(|(id, _)| *id);
        let chat_title = chat.map(|(_, title)| title).unwrap_or_default();
        let picture = chat_id.and_then(|id| self.group_call_stream_picture(id));
        let placeholder = chat_id.and_then(|id| self.group_call_stream_photo(id, cx));
        let playing = call.is_joined && call.stream != StreamState::Unsupported;

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

        let label = stream_label(call, &chat_title);
        let title = if call.title.is_empty() {
            chat_title
        } else {
            call.title.clone()
        };
        let viewers = i64::from(call.viewer_count.max(1));
        let viewers = trn_with(
            keys::GROUP_CALL_RTMP_VIEWERS,
            viewers,
            &[("count", &quill::premium_hub::group_digits(viewers))],
        );

        let element = div()
            .id("group-call-stream")
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(if full { black() } else { rgb(BG).into() })
            .text_color(white())
            .child(viewport)
            // The title band: a double-click toggles full screen (the
            // window's maximize request, which an RTMP panel turns into
            // full screen).
            .when(!full, |this| {
                this.child(
                    div()
                        .id("group-call-stream-title-band")
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .h(px(VIDEO_TOP))
                        .on_click(cx.listener(|_, event: &ClickEvent, window, cx| {
                            if event.click_count() == 2 {
                                toggle_stream_full(window);
                                cx.notify();
                            }
                        })),
                )
            })
            .children(label.map(label_view))
            .child(title_row(title, viewers, full, shown))
            .when(shown > 0., |this| {
                this.child(self.group_call_stream_bar(call, full, shown, cx))
            })
            .into_any_element();
        StreamBody { element, animating }
    }

    /// The bottom bar: the "…" menu for managers, the full screen button,
    /// Leave. Slides in from below with `shown`.
    fn group_call_stream_bar(
        &self,
        call: &ActiveGroupCall,
        full: bool,
        shown: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let round = |id: &'static str, label: SharedString, size: f32, bg: Background| {
            div()
                .id(id)
                .size(px(size))
                .rounded_full()
                .bg(bg)
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .role(Role::Button)
                .aria_label(label)
                .hover(|style| style.opacity(0.85))
        };
        let menu = call.can_be_managed.then(|| {
            div()
                .w(px(60.))
                .h(px(BAR_HEIGHT))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .size(px(44.))
                        .rounded_full()
                        .bg(rgba(ICON_BG))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(self.group_call_menu(call, cx)),
                )
        });
        let expand = div()
            .w(px(68.))
            .h(px(BAR_HEIGHT))
            .flex()
            .items_center()
            .justify_center()
            .child(
                round(
                    "group-call-stream-fullscreen",
                    SharedString::from(tr(keys::Q_MISC_TOGGLE_FULL_SCREEN)),
                    42.,
                    linear_gradient(
                        135.,
                        linear_color_stop(rgb(BUTTON_FROM), 0.),
                        linear_color_stop(rgb(BUTTON_TO), 1.),
                    ),
                )
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
        let leave = div()
            .w(px(60.))
            .h(px(BAR_HEIGHT))
            .flex()
            .items_center()
            .justify_center()
            .child(
                round(
                    "group-call-stream-leave",
                    SharedString::from(tr(keys::Q_PEOPLE_LEAVE)),
                    44.,
                    rgba(LEAVE_BG).into(),
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    leave_stream_full(window);
                    this.leave_active_group_call(cx);
                }))
                .child(
                    Icon::new(IconName::Close)
                        .with_size(px(22.))
                        .text_color(white()),
                ),
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
                    .children(menu)
                    .child(expand)
                    .child(leave),
            )
    }

    /// The newest stream picture of `chat_id` as an image, made once per
    /// frame sequence.
    fn group_call_stream_picture(&mut self, chat_id: i64) -> Option<Arc<RenderImage>> {
        let seq = match self.live.as_ref() {
            Some(live) => live.driver.latest_stream_frame_seq(chat_id)?,
            None => self.group_call.stream.demo_frame.as_ref()?.seq,
        };
        if let Some((cached_chat, cached_seq, image)) = self.group_call.stream.image.as_ref()
            && *cached_chat == chat_id
            && *cached_seq == seq
        {
            return Some(image.clone());
        }
        let frame = match self.live.as_ref() {
            Some(live) => live.driver.latest_stream_frame(chat_id)?,
            None => self.group_call.stream.demo_frame.clone()?,
        };
        let image = Self::video_render_image(&frame)?;
        if let Some((_, _, old)) =
            self.group_call
                .stream
                .image
                .replace((chat_id, frame.seq, image.clone()))
        {
            super::image_budget::retire_all([old]);
        }
        Some(image)
    }

    /// The chat's photo, blurred once per photo off the UI thread.
    fn group_call_stream_photo(
        &mut self,
        chat_id: i64,
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
            super::image_budget::retire_all([old]);
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
fn stream_label(call: &ActiveGroupCall, chat_title: &str) -> Option<String> {
    match call.stream {
        StreamState::NoStream if call.rtmp_url.as_deref().is_some_and(|url| !url.is_empty()) => {
            Some(tr(keys::GROUP_CALL_NO_STREAM_ADMIN).to_string())
        }
        StreamState::NoStream => Some(tr_with(
            keys::GROUP_CALL_NO_STREAM,
            &[("group", chat_title)],
        )),
        StreamState::Unsupported => Some(tr(keys::Q_OPS_STREAM_UNSUPPORTED).to_string()),
        StreamState::Connecting | StreamState::Live | StreamState::Ended => None,
    }
}

/// tdesktop's empty-stream label: semibold `groupCallMemberNotJoinedStatus`
/// text centered in a rounded box, as wide as the text allows (at most
/// 380 px plus padding, 12 px from the window's sides), a third of the
/// way down.
fn label_view(text: String) -> AnyElement {
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
                .mx(px(LABEL_PADDING))
                .max_w(px(LABEL_WIDTH + 2. * LABEL_PADDING))
                .min_w(px(96. + 2. * LABEL_PADDING))
                .p(px(LABEL_PADDING))
                .rounded(px(12.))
                .bg(rgba(CONTROLS_BG))
                .text_size(px(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(MEMBER_NOT_JOINED))
                .text_center()
                .child(text),
        )
        .child(div().flex_grow(2.))
        .into_any_element()
}

/// The title, a bullet and the viewer count, centered in the title band.
/// Full screen it slides up with the controls on a backdrop rounded at
/// the bottom (`refreshTitleBackground`).
fn title_row(title: String, viewers: String, full: bool, shown: f32) -> impl IntoElement {
    // The text sits 3 px down the 24 px band; full screen the backdrop
    // reaches the top edge and slides out by its own height.
    let offset = if full { -25. * (1. - shown) } else { 0. };
    div()
        .absolute()
        .top(px(offset))
        .left(px(48.))
        .right(px(48.))
        .flex()
        .justify_center()
        .child(
            div()
                .id("group-call-stream-title")
                .debug_selector(|| "group-call-stream-title".into())
                .min_w_0()
                .pt(px(3.))
                .pb(px(4.))
                .px(px(8.))
                .when(full, |this| this.rounded_b(px(6.)).bg(rgba(CONTROLS_BG)))
                .flex()
                .items_center()
                .gap(px(4.))
                .text_size(px(14.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(div().min_w_0().truncate().child(title))
                .child(div().flex_none().child("\u{2022}"))
                .child(div().flex_none().child(viewers)),
        )
}

#[cfg(feature = "demo-capture")]
#[path = "group_call_stream_demo.rs"]
mod demo;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "group_call_stream_tests.rs"]
mod ui_tests;
