//! Parts of the viewer overlay: the zoom row and the saved-file toast
//! (the video controller is `controller.rs`).

use super::*;
use quill::lang::{keys, tr};

impl QuillApp {
    /// Parity slice 5: zoom controls share a row with the video
    /// transport — − / % / + / Reset, then Play/Pause + elapsed/total.
    pub(super) fn viewer_zoom_controls(
        zoom: ViewerZoom,
        row_id: u64,
        cx: &mut Context<Self>,
    ) -> Div {
        let zoom_pct = format!("{}%", zoom.percent().round() as i32);
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                Button::new(("media-viewer-zoom-out", row_id))
                    .label("−")
                    .accessibility_label(tr(keys::MEDIA_ZOOM_OUT))
                    .ghost()
                    .text_color(gpui_kit::white())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.viewer_zoom_step(false, cx);
                    })),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(gpui_kit::white())
                    .child(zoom_pct),
            )
            .child(
                Button::new(("media-viewer-zoom-in", row_id))
                    .label("+")
                    .accessibility_label(tr(keys::MEDIA_ZOOM_IN))
                    .ghost()
                    .text_color(gpui_kit::white())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.viewer_zoom_step(true, cx);
                    })),
            )
            .child(
                Button::new(("media-viewer-zoom-reset", row_id))
                    .label(tr(keys::Q_AV_RESET))
                    .ghost()
                    .text_color(gpui_kit::white())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.viewer_reset_zoom(cx);
                    })),
            )
    }

    /// tdesktop `showSaveMsgToast`: where the saved file went, with a
    /// link that shows it in the file manager.
    pub(super) fn viewer_saved_toast(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.viewer.extra.saved_toast.clone().map(|toast| {
            div()
                .absolute()
                .top(px(VIEWER_TOP_BAR + 12.))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    div()
                        .id("media-viewer-saved-toast")
                        .occlude()
                        .role(Role::Status)
                        .aria_label(format!(
                            "{}{}{}",
                            toast.text.before, toast.text.folder, toast.text.after
                        ))
                        .max_w(px(520.))
                        .pl_3()
                        .pr_1()
                        .py_1()
                        .rounded_full()
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().popover)
                        .text_color(cx.theme().popover_foreground)
                        .shadow_md()
                        .text_sm()
                        .flex()
                        .items_center()
                        .child(toast.text.before.clone())
                        .child(
                            Button::new("media-viewer-saved-folder")
                                .label(toast.text.folder.clone())
                                .link()
                                .small()
                                .tooltip(tr(keys::Q_AV_SHOW_IN_FOLDER_2))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.reveal_saved_toast_file(cx);
                                })),
                        )
                        .child(toast.text.after.clone()),
                )
                .into_any_element()
        })
    }
}
