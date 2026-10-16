//! What the viewer draws over the picture besides the picture itself: the
//! waiting spinner (after `kWaitingShowDelay`), the download ring (a click
//! cancels), the download disc (a click starts it) and the "can't be
//! played" plate. The thumbnail stays under all of them; the viewer never
//! closes on its own.
//!
//! Live pictures (`live_overlay.rs` decides which and when) add: the
//! placeholder with its spinner while opening, the dimmed picture with
//! the spinner and "Connecting…" while stalled, tdesktop's paused shadow
//! and icon, the dimmed picture with "Live stream ended" and Leave, and
//! the failed card with Retry. [`stream_label_box`] is the stream
//! window's label on the controls backdrop (nobody broadcasting, Join).
#![deny(clippy::indexing_slicing)]

use super::live::CONTROLS_BG;
use super::live_overlay::LiveOverlay;
use super::metrics::{PROGRESS_RING, SPINNER};
use super::{Handler, overlay_button, panel_background};
use crate::ui::group_call_panel::MEMBER_NOT_JOINED;
use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::progress::ProgressCircle;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme, Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr};

/// What shows over the picture.
pub(crate) enum PictureState {
    /// The clip waits for data (shown only past the delay).
    Waiting,
    /// The clip is downloading: `progress` 0–1 when known; the click
    /// cancels.
    Downloading {
        progress: Option<f32>,
        on_cancel: Handler,
    },
    /// The clip is not local: the click downloads it.
    Download { on_start: Handler },
    /// The file can't play: `detail` says why; `open` offers the system
    /// player when the file is local.
    Failed {
        detail: SharedString,
        open: Option<Handler>,
    },
    /// A live picture with no frame yet: `placeholder` (the blurred
    /// photo or the initials) fills it, the spinner past its delay.
    LiveOpening {
        placeholder: Option<AnyElement>,
        spinner: bool,
    },
    /// A stalled live picture, dimmed under the spinner.
    LiveStalled { connecting: bool },
    /// The sender paused: tdesktop's shadow and pause icon over the
    /// blurred picture, and the text when `wide`.
    LivePaused { wide: bool },
    /// The live picture is over: dimmed, with Leave when the host has it.
    LiveEnded { on_leave: Option<Handler> },
    /// The live picture failed: the reason, and Retry when the host can.
    LiveFailed {
        detail: Option<SharedString>,
        on_retry: Option<Handler>,
    },
}

/// What a host hands over for its live overlays.
#[derive(Default)]
pub(crate) struct LiveOverlayParts {
    /// The blurred photo or initials while opening or idle.
    pub(crate) placeholder: Option<AnyElement>,
    /// The paused text shows (a wide view: the stage, a stream).
    pub(crate) wide: bool,
    pub(crate) on_leave: Option<Handler>,
    pub(crate) on_retry: Option<Handler>,
    pub(crate) detail: Option<SharedString>,
}

impl PictureState {
    /// The state for a live overlay, `None` when nothing shows.
    pub(crate) fn live(overlay: LiveOverlay, parts: LiveOverlayParts) -> Option<Self> {
        Some(match overlay {
            LiveOverlay::None => return None,
            LiveOverlay::Opening { spinner } => PictureState::LiveOpening {
                placeholder: parts.placeholder,
                spinner,
            },
            LiveOverlay::Idle => PictureState::LiveOpening {
                placeholder: Some(parts.placeholder?),
                spinner: false,
            },
            LiveOverlay::Stalled { connecting } => PictureState::LiveStalled { connecting },
            LiveOverlay::Paused => PictureState::LivePaused { wide: parts.wide },
            LiveOverlay::Ended => PictureState::LiveEnded {
                on_leave: parts.on_leave,
            },
            LiveOverlay::Failed => PictureState::LiveFailed {
                detail: parts.detail,
                on_retry: parts.on_retry,
            },
        })
    }
}

/// tdesktop `Viewport::kShadowMaxAlpha` (80 of 255) over a paused tile.
const PAUSED_SHADOW: f32 = 80. / 255.;
/// A stalled picture shows at 60 %.
const STALLED_SHADE: f32 = 0.4;
/// An ended picture is dimmed further.
const ENDED_SHADE: f32 = 0.6;
/// The stream window's label (`setupEmptyRtmp`): `st::groupCallWidth / 30`
/// padding, at most `st::groupCallWidth` of text, 24 px from each side.
const LABEL_PADDING: f32 = 12.;
const LABEL_WIDTH: f32 = 380.;
const LABEL_SIDES: f32 = 48.;

/// The stream window's label (tdesktop `setupEmptyRtmp`): semibold
/// `groupCallMemberNotJoinedStatus` text centered in a rounded box on the
/// controls backdrop, `min(width - 48, 380) + 24` wide, a third of the
/// way down. `action` goes under the text (Quill's Join).
pub(crate) fn stream_label_box(
    text: String,
    window_width: f32,
    action: Option<AnyElement>,
) -> AnyElement {
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

/// A layer filling the picture.
fn fill() -> Div {
    div().absolute().inset_0()
}

fn spinner(row_id: u64, white: Hsla) -> AnyElement {
    div()
        .id(("live-spinner", row_id))
        .debug_selector(|| "live-spinner".into())
        .role(Role::ProgressIndicator)
        .aria_label(tr(keys::GROUP_CALL_CONNECTING))
        .child(Spinner::new().with_size(SPINNER).color(white))
        .into_any_element()
}

/// The live states' layers (see [`PictureState`]); any other state is
/// handed back.
fn live_content(row_id: u64, state: PictureState, cx: &App) -> Result<AnyElement, PictureState> {
    let white = gpui_kit::white();
    Ok(match state {
        PictureState::LiveOpening {
            placeholder,
            spinner: shown,
        } => fill()
            .debug_selector(|| "live-opening".into())
            .children(placeholder)
            .when(shown, |this| {
                this.child(
                    fill()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(spinner(row_id, white)),
                )
            })
            .into_any_element(),
        PictureState::LiveStalled { connecting } => fill()
            .debug_selector(|| "live-stalled".into())
            .bg(gpui_kit::black().opacity(STALLED_SHADE))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .child(spinner(row_id, white))
            .when(connecting, |this| {
                this.child(
                    div()
                        .debug_selector(|| "live-connecting".into())
                        .text_sm()
                        .font_semibold()
                        .text_color(white)
                        .child(tr(keys::GROUP_CALL_CONNECTING)),
                )
            })
            .into_any_element(),
        PictureState::LivePaused { wide } => fill()
            .debug_selector(|| "live-paused".into())
            .bg(gpui_kit::black().opacity(PAUSED_SHADOW))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.))
            .text_color(white)
            .child(Icon::new(IconName::Pause).with_size(px(if wide { 40. } else { 28. })))
            .when(wide, |this| {
                this.child(
                    div()
                        .text_size(px(14.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(tr(keys::GROUP_CALL_VIDEO_PAUSED)),
                )
            })
            .into_any_element(),
        PictureState::LiveEnded { on_leave } => fill()
            .id(("live-ended", row_id))
            .debug_selector(|| "live-ended".into())
            .occlude()
            .role(Role::Status)
            .bg(gpui_kit::black().opacity(ENDED_SHADE))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .child(
                div()
                    .text_base()
                    .font_semibold()
                    .text_color(white)
                    .child(tr(keys::Q_AV_LIVE_STREAM_ENDED)),
            )
            .when_some(on_leave, |this, leave| {
                this.child(
                    Button::new(("live-ended-leave", row_id))
                        .label(tr(keys::GROUP_CALL_LEAVE))
                        .small()
                        .custom(overlay_button(cx).color(panel_background()))
                        .on_click(move |_, window, cx| leave(window, cx)),
                )
            })
            .into_any_element(),
        PictureState::LiveFailed { detail, on_retry } => div()
            .id(("live-failed", row_id))
            .debug_selector(|| "live-failed".into())
            .occlude()
            .role(Role::Alert)
            .max_w(px(320.))
            .px_4()
            .py_3()
            .rounded(cx.theme().radius_lg)
            .bg(panel_background())
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .text_center()
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(white)
                    .child(tr(keys::Q_AV_LIVE_STREAM_FAILED)),
            )
            .children(
                detail.map(|detail| div().text_xs().text_color(white.opacity(0.8)).child(detail)),
            )
            .when_some(on_retry, |this, retry| {
                this.child(
                    Button::new(("live-failed-retry", row_id))
                        .label(tr(keys::Q_AV_RETRY))
                        .small()
                        .custom(overlay_button(cx))
                        .mt_1()
                        .on_click(move |_, window, cx| retry(window, cx)),
                )
            })
            .into_any_element(),
        other => return Err(other),
    })
}

/// The download percentage the ring shows ("42%"), `None` while unknown.
pub(crate) fn progress_label(progress: Option<f32>) -> Option<String> {
    let progress = progress.filter(|p| p.is_finite())?;
    Some(format!(
        "{}%",
        (progress.clamp(0.0, 1.0) * 100.0).round() as u32
    ))
}

/// The layer over the picture, filling it and centering its content.
#[derive(IntoElement)]
pub(crate) struct PictureOverlay {
    row_id: u64,
    state: PictureState,
}

impl PictureOverlay {
    pub(crate) fn new(row_id: u64, state: PictureState) -> Self {
        Self { row_id, state }
    }
}

impl RenderOnce for PictureOverlay {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let row_id = self.row_id;
        let white = gpui_kit::white();
        let layer = div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center();
        let state = match live_content(row_id, self.state, cx) {
            Ok(content) => return layer.child(content),
            Err(state) => state,
        };
        let content: AnyElement = match state {
            PictureState::Waiting => div()
                .id(("viewer-waiting", row_id))
                .test_support()
                .role(Role::ProgressIndicator)
                .aria_label(tr(keys::Q_AV_LOADING_VIDEO))
                .child(Spinner::new().with_size(SPINNER).color(white))
                .into_any_element(),
            PictureState::Downloading {
                progress,
                on_cancel,
            } => {
                let label = progress_label(progress);
                div()
                    .id(("viewer-download-ring", row_id))
                    .test_support()
                    .occlude()
                    .cursor_pointer()
                    .size(PROGRESS_RING)
                    .rounded_full()
                    .bg(panel_background())
                    .role(Role::Button)
                    .aria_label(tr(keys::Q_AV_CANCEL_DOWNLOAD))
                    .on_click(move |_, window, cx| on_cancel(window, cx))
                    .child(
                        ProgressCircle::new(("viewer-download-progress", row_id))
                            .size(PROGRESS_RING)
                            .color(white)
                            .loading(progress.is_none())
                            .value(progress.unwrap_or(0.0).clamp(0.0, 1.0) * 100.0)
                            .accessibility_label(tr(keys::SR_MESSAGE_DOWNLOADING))
                            .child(
                                div()
                                    .size_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(white)
                                    .children(label),
                            ),
                    )
                    .into_any_element()
            }
            PictureState::Download { on_start } => Button::new(("viewer-download", row_id))
                .icon(IconName::ArrowDown)
                .custom(overlay_button(cx).color(panel_background()))
                .rounded(PROGRESS_RING / 2.)
                .with_size(PROGRESS_RING)
                .tooltip(tr(keys::MEDIA_DOWNLOAD))
                .accessibility_label(tr(keys::MEDIA_DOWNLOAD))
                .on_click(move |_, window, cx| on_start(window, cx))
                .into_any_element(),
            PictureState::Failed { detail, open } => div()
                .id(("viewer-failed", row_id))
                .test_support()
                .occlude()
                .role(Role::Alert)
                .max_w(px(320.))
                .px_4()
                .py_3()
                .rounded(cx.theme().radius_lg)
                .bg(panel_background())
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .text_center()
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .text_color(white)
                        .child(tr(keys::Q_AV_THIS_VIDEO_CANT_BE_PLAYED_2)),
                )
                .child(div().text_xs().text_color(white.opacity(0.8)).child(detail))
                .when_some(open, |this, open| {
                    this.child(
                        Button::new(("media-viewer-open-externally", row_id))
                            .label(tr(keys::Q_AV_OPEN_EXTERNALLY))
                            .small()
                            .custom(overlay_button(cx))
                            .mt_1()
                            .on_click(move |_, window, cx| open(window, cx)),
                    )
                })
                .into_any_element(),
            // Drawn by `live_content`.
            PictureState::LiveOpening { .. }
            | PictureState::LiveStalled { .. }
            | PictureState::LivePaused { .. }
            | PictureState::LiveEnded { .. }
            | PictureState::LiveFailed { .. } => div().into_any_element(),
        };
        layer.child(content)
    }
}
