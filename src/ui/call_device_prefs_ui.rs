//! The "Other" rows of Settings > Calls: "Accept calls on this device" for
//! the current session (`toggleSessionCanAcceptCalls`) and "Open system
//! sound preferences". Rules in `quill::call_device_prefs`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_base::Disableable;
use gpui_kit::component::button::Button;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, Sizable};
use gpui_kit::*;
use quill::call_device_prefs::{
    ACCEPT_CALLS_TITLE, SYSTEM_PREFS_TITLE, current_session_accepts_calls, system_sound_target,
};
use quill::lang::{keys, tr};

impl QuillApp {
    pub(super) fn call_device_prefs_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.session().map(|session| &session.settings);
        let current = settings
            .and_then(|settings| settings.sessions.as_deref())
            .and_then(current_session_accepts_calls);
        let mutating = settings.is_some_and(|settings| settings.sessions_mutating);
        // The list arrives once; ask for it when this section first shows.
        let needs_fetch = self.live.is_some()
            && settings.is_some_and(|settings| {
                settings.sessions.is_none()
                    && !settings.sessions_loading
                    && settings.sessions_error.is_none()
            });
        if needs_fetch {
            // `maybe_fetch_active_sessions` is guarded, so a render that
            // queues this twice sends one request.
            cx.spawn(async move |this, cx| {
                let _ = this.update(cx, |this, cx| {
                    if let Some(live) = this.live.as_mut() {
                        let _ = live.driver.maybe_fetch_active_sessions();
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        let mut block = div().flex().flex_col().gap_1().px_2().py_1();
        if let Some((session_id, on)) = current {
            block = block.child(
                div()
                    .id("call-accept-here")
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Switch::new("call-accept-here-switch")
                            .checked(on)
                            .disabled(mutating)
                            .accessibility_label(ACCEPT_CALLS_TITLE)
                            .on_click(cx.listener(move |this, &requested, _, cx| {
                                if requested != on {
                                    this.toggle_session_calls(session_id, cx);
                                }
                            })),
                    )
                    .child(div().text_sm().child(ACCEPT_CALLS_TITLE)),
            );
        }
        if let Some(target) = system_sound_target() {
            block = block.child(
                div().flex().child(
                    Button::new("call-system-sound-prefs")
                        .label(SYSTEM_PREFS_TITLE)
                        .small()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let failure = tr(keys::Q_OPS_COULDNT_OPEN_THE_SYSTEM_SOUND);
                            this.open_system_settings(target, failure, cx);
                        })),
                ),
            );
        }
        block.text_color(cx.theme().foreground).into_any_element()
    }
}
