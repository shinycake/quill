//! The Notifications page (tdesktop "Notifications"): alerts and sounds,
//! the defaults for each kind of chat, events, the badge, how a
//! notification looks and where it shows, and Reset all at the end.
#![deny(clippy::indexing_slicing)]

use super::QuillApp;
use super::rows::{danger_row, group};
use crate::ui::dialogs::NotificationsConfirm;
use gpui_kit::assets::IconName;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::telegram::envelope::{NotificationSettingsScope, NotificationSound};

impl QuillApp {
    /// Opens Settings on this page and asks for what it shows.
    pub(crate) fn open_notification_settings(&mut self, cx: &mut Context<Self>) {
        self.open_settings(Some(quill::settings_catalog::Page::Notifications));
        self.fetch_notification_settings();
        cx.notify();
    }

    pub(super) fn fetch_notification_settings(&mut self) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.maybe_fetch_scope_notification_settings();
            let _ = live.driver.maybe_fetch_notification_sounds();
            for scope in NotificationSettingsScope::ALL {
                let _ = live.driver.maybe_fetch_notification_exceptions(scope);
            }
        }
    }

    /// Leaving the page drops its open pickers and the pending reset.
    pub(super) fn leave_notification_settings(&mut self) {
        self.notify.defaults_sound_picker = None;
        self.notify.defaults_exceptions_scope = None;
        self.notify.notifications_confirm = None;
    }

    pub(super) fn notifications_page(&mut self, cx: &mut Context<Self>) -> Div {
        self.ensure_scope_volume_sliders(cx);
        self.ensure_notification_volume_slider(cx);
        let saved_sounds: Vec<NotificationSound> = self
            .session()
            .map(|s| s.settings.saved_notification_sounds.clone())
            .unwrap_or_default();
        let mut alerts = vec![
            self.desktop_notifications_section(cx),
            self.inapp_sounds_section(cx),
            self.notification_volume_section(cx),
            self.attention_section(cx),
        ];
        alerts.extend(self.notify_from_all_section(cx));
        let mut chats = vec![
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(tr(keys::Q_PREFS_USED_WHEN_A_CHAT_KEEPS))
                .into_any_element(),
        ];
        for scope in NotificationSettingsScope::ALL {
            chats.push(self.scope_settings_section(cx, scope, &saved_sounds));
        }
        chats.push(self.reaction_settings_section(cx, &saved_sounds));
        let mut look: Vec<AnyElement> = self.notify_view_section(cx).into_iter().collect();
        look.extend(self.notify_options_section(cx));
        let confirm = self.notify.notifications_confirm.map(|confirm| {
            self.notifications_confirm_banner(confirm, cx)
                .into_any_element()
        });
        div()
            .flex()
            .flex_col()
            .gap_6()
            .w_full()
            .child(stack(alerts))
            .child(stack(chats))
            .child(self.events_section(cx))
            .child(self.badge_counter_section(cx))
            .child(stack(look))
            .child(group(
                None,
                [danger_row(
                    "settings-row-reset-notifications",
                    IconName::RotateCcw,
                    tr(keys::Q_PREFS_RESET_ALL),
                    cx.listener(|this, _, _, cx| {
                        this.notify.notifications_confirm = Some(NotificationsConfirm::ResetAll);
                        cx.notify();
                    }),
                    cx,
                )],
            ))
            .children(confirm)
    }
}

/// These sections draw their own outlines: they stack, unwrapped.
fn stack(sections: Vec<AnyElement>) -> Div {
    div().flex().flex_col().gap_3().children(sections)
}
