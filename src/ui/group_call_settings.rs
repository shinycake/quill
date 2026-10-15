//! The voice chat window's Settings box (tdesktop: the call menu's
//! "Settings", `calls_group_menu.cpp`, opening `SettingsBox` in
//! `calls_group_settings.cpp`). It holds the "Enable Noise Suppression"
//! switch, the only place it lives as in tdesktop, applied to your
//! group call microphone at once (1:1 calls always denoise). The other
//! rows of tdesktop's box live where Quill already has them: devices in
//! the call window's device menus, the microphone test and push-to-talk
//! in Settings > Calls.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::switch::Switch;
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
        let noise_row = self.noise_suppression_row(cx);
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

    /// Whether group calls denoise your microphone (the saved switch; on
    /// by default). 1:1 calls always do.
    pub(super) fn call_noise_suppression(&self) -> bool {
        self.session()
            .map(|session| session.calls.prefs.call_noise_suppression)
            .unwrap_or(true)
    }

    /// The "Enable Noise Suppression" row (tdesktop
    /// `lng_group_call_noise_suppression`), a kit `Switch` with its label.
    fn noise_suppression_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.call_noise_suppression();
        let label = tr(keys::GROUP_CALL_NOISE_SUPPRESSION);
        div()
            .id("group-call-noise-suppression")
            .flex()
            .items_center()
            .gap_2()
            .py_1()
            .child(
                Switch::new("group-call-noise-suppression-switch")
                    .checked(on)
                    .accessibility_label(label)
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.set_call_noise_suppression(on, cx);
                    })),
            )
            .child(div().text_sm().child(label))
            .into_any_element()
    }

    /// Save the switch and apply it to a running group call's microphone
    /// now. 1:1 calls always denoise, as in tdesktop.
    pub(super) fn set_call_noise_suppression(&mut self, on: bool, cx: &mut Context<Self>) {
        self.set_call_pref(|prefs| prefs.call_noise_suppression = on, cx);
        if let Some(live) = self.live.as_mut() {
            live.driver.apply_call_noise_suppression();
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "group_call_settings_tests.rs"]
mod tests;
