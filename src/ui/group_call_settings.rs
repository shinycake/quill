//! The voice chat window's Settings box (tdesktop: the call menu's
//! "Settings", `calls_group_menu.cpp`, opening `SettingsBox` in
//! `calls_group_settings.cpp`). It holds the "Enable Noise Suppression"
//! switch, applied to your microphone at once. The other rows of
//! tdesktop's box live where Quill already has them: devices in the
//! call window's device menus, the microphone test and push-to-talk in
//! Settings > Calls.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::*;
use quill::lang::{keys, tr};

impl QuillApp {
    pub(super) fn open_group_call_settings(&mut self, cx: &mut Context<Self>) {
        self.group_call.settings_open = true;
        cx.notify();
    }

    pub(super) fn close_group_call_settings(&mut self, cx: &mut Context<Self>) {
        self.group_call.settings_open = false;
        cx.notify();
    }

    /// The Settings box over the voice chat window, while open.
    pub(super) fn group_call_settings_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.group_call.settings_open {
            return None;
        }
        let noise_row = self.noise_suppression_row("group-call-noise-suppression", cx);
        Some(
            div()
                .id("group-call-settings")
                .absolute()
                .inset_0()
                .bg(cx.theme().overlay)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .w(px(320.))
                        .rounded(cx.theme().radius_lg)
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().background)
                        .text_color(cx.theme().foreground)
                        .p_4()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(tr(keys::GROUP_CALL_SETTINGS_TITLE)),
                        )
                        .child(noise_row)
                        .child(
                            div().flex().justify_end().child(
                                Button::new("group-call-settings-done")
                                    .label(tr(keys::BOX_DONE))
                                    .primary()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.close_group_call_settings(cx)
                                    })),
                            ),
                        ),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "group_call_settings_tests.rs"]
mod tests;
