//! What the viewer draws over the picture besides the picture itself: the
//! waiting spinner (after `kWaitingShowDelay`), the download ring (a click
//! cancels), the download disc (a click starts it) and the "can't be
//! played" plate. The thumbnail stays under all of them; the viewer never
//! closes on its own.
#![deny(clippy::indexing_slicing)]

use super::metrics::{PROGRESS_RING, SPINNER};
use super::{Handler, overlay_button, panel_background};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::progress::ProgressCircle;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme, Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

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
        let content: AnyElement = match self.state {
            PictureState::Waiting => div()
                .id(("viewer-waiting", row_id))
                .test_support()
                .role(Role::ProgressIndicator)
                .aria_label("Loading video")
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
                    .aria_label("Cancel download")
                    .on_click(move |_, window, cx| on_cancel(window, cx))
                    .child(
                        ProgressCircle::new(("viewer-download-progress", row_id))
                            .size(PROGRESS_RING)
                            .color(white)
                            .loading(progress.is_none())
                            .value(progress.unwrap_or(0.0).clamp(0.0, 1.0) * 100.0)
                            .accessibility_label("Downloading")
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
                .tooltip("Download")
                .accessibility_label("Download")
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
                        .child("This video can't be played"),
                )
                .child(div().text_xs().text_color(white.opacity(0.8)).child(detail))
                .when_some(open, |this, open| {
                    this.child(
                        Button::new(("media-viewer-open-externally", row_id))
                            .label("Open externally")
                            .small()
                            .custom(overlay_button(cx))
                            .mt_1()
                            .on_click(move |_, window, cx| open(window, cx)),
                    )
                })
                .into_any_element(),
        };
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .child(content)
    }
}
