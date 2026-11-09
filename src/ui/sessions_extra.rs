//! B13: the Active Sessions additions (tdesktop `Settings::Sessions`):
//! the "Terminate old sessions" period and renaming this device.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use crate::ui::user_error::user_error_for;
use gpui_kit::component::input::Input;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr};

impl QuillApp {
    pub(super) fn set_session_ttl(&mut self, days: i32, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live.driver.set_inactive_session_ttl(days) {
                self.set_note(StatusNote::error(user_error_for(
                    keys::Q_NOTES_FAILED_SESSION_TIMEOUT,
                    &err,
                )));
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.settings.privacy_data.inactive_session_ttl_days = Some(days);
        }
        cx.notify();
    }

    pub(super) fn begin_rename_device(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self
            .session()
            .map(|s| s.settings.device_prefs.custom_device_model.clone())
            .unwrap_or_default();
        self.privacy
            .extra
            .rename_input
            .update(cx, |input, cx| input.set_value(current, window, cx));
        self.privacy.extra.renaming_device = true;
        let input = self.privacy.extra.rename_input.clone();
        window.defer(cx, move |window, cx| {
            input.update(cx, |input, cx| input.focus(window, cx));
        });
        cx.notify();
    }

    /// tdesktop `RenameBox`: the device name field; the session box
    /// footer holds Save and Cancel.
    pub(super) fn rename_device_fields(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_PREFS_DEVICE_NAME)),
            )
            .child(
                Input::new(&self.privacy.extra.rename_input)
                    .aria_label(tr(keys::Q_PREFS_DEVICE_NAME)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_PREFS_THE_NEW_NAME_APPLIES_AFTER)),
            )
    }

    pub(super) fn save_device_name(&mut self, cx: &mut Context<Self>) {
        let raw = self.privacy.extra.rename_input.read(cx).value().to_string();
        let prefs = quill::settings::DevicePrefs {
            custom_device_model: quill::settings::clean_device_model(&raw),
        };
        if let Some(live) = self.live.as_mut() {
            live.driver.session.settings.device_prefs = prefs;
            live.driver.save_device_prefs();
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.settings.device_prefs = prefs;
            self.set_note(StatusNote::silent(tr(
                keys::Q_PREFS_DEMO_DEVICE_NAME_IS_NOT,
            )));
        }
        self.privacy.extra.renaming_device = false;
        cx.notify();
    }
}
