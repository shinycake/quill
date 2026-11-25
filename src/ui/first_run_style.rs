//! The onboarding's last card: pick a layout (Classic or Floating) before
//! signing in. The card shows `QuillApp::layout_picker`, the same element
//! Settings › Appearance shows, so a pick here is the pref the Settings card
//! writes (docs/decisions/floating-layout.md).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::*;
use quill::lang::{keys, tr};

impl QuillApp {
    /// The style card: heading, the picker, where to change it later, and
    /// the button that finishes onboarding.
    pub(super) fn first_run_style(&self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        super::first_run_ui::card()
            .child(super::first_run_ui::heading(tr(
                keys::Q_FIRST_RUN_STYLE_TITLE,
            )))
            .child(self.layout_picker(cx))
            .children(self.layout_wallpaper_offer(true, cx))
            .child(
                div()
                    .text_sm()
                    .text_center()
                    .text_color(muted)
                    .child(tr(keys::Q_FIRST_RUN_STYLE_LATER)),
            )
            .child(
                div()
                    .w_full()
                    .debug_selector(|| "first-run-start".into())
                    .child(
                        Button::new("first-run-start")
                            .label(tr(keys::START_MSGS))
                            .primary()
                            .w_full()
                            .on_click(cx.listener(|this, _, _, cx| this.finish_first_run(cx))),
                    ),
            )
    }
}
