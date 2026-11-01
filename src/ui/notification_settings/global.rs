//! App-wide notification settings: badge counter, desktop notifications,
//! in-app sounds, the Notifications page sections, and Reset All.

use super::*;
use crate::ui::status_note::StatusNote;
use crate::ui::user_error::user_error_for;
use quill::lang::{keys, tr, tr_with};

impl QuillApp {
    /// Slice parity:chatlist-badge-settings: update one badge-counter
    /// pref in the session and persist it to the account dir (via
    /// `ConnectDriver::save_badge_prefs`).
    pub(in crate::ui) fn set_badge_pref(
        &mut self,
        update: impl FnOnce(&mut BadgePrefs),
        cx: &mut Context<Self>,
    ) {
        let mut prefs = self
            .session()
            .map(|session| session.settings.badge_prefs)
            .unwrap_or_default();
        update(&mut prefs);
        if let Some(live) = self.live.as_mut() {
            live.driver.session.settings.badge_prefs = prefs;
            if let Err(err) = live.driver.save_badge_prefs() {
                self.connection.set_note(StatusNote::silent(tr_with(
                    keys::Q_PREFS_COULDN_T_SAVE_BADGE_SETTINGS,
                    &[("err", &err.to_string())],
                )));
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.settings.badge_prefs = prefs;
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_PREFS_DEMO_BADGE_SETTINGS_ARE_NOT,
            )));
        }
        cx.notify();
    }

    /// Parity slice: in-app notification sounds toggle (tdesktop "Play
    /// sounds"). Writes through to prefs (persist) and updates the
    /// Session mirror immediately.
    /// tdesktop "Desktop notifications" (`desktopNotify`), also flipped from
    /// the tray menu's "Disable/Enable notifications".
    pub(crate) fn set_desktop_notifications(&mut self, on: bool, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.settings.desktop_notifications = on;
            live.driver.save_desktop_notifications();
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.settings.desktop_notifications = on;
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_PREFS_DEMO_DESKTOP_NOTIFICATIONS_ARE_NOT,
            )));
        }
        quill::wake::CHROME.poke();
        cx.notify();
    }

    pub(crate) fn desktop_notifications_enabled(&self) -> bool {
        self.session()
            .is_none_or(|s| s.settings.desktop_notifications)
    }

    pub(crate) fn notification_sounds_enabled(&self) -> bool {
        self.session()
            .is_none_or(|s| s.settings.inapp_sounds_enabled)
    }

    pub(crate) fn set_inapp_sounds_enabled(&mut self, on: bool, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.settings.inapp_sounds_enabled = on;
            live.driver.save_inapp_sounds_enabled();
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.settings.inapp_sounds_enabled = on;
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_PREFS_DEMO_IN_APP_SOUNDS_ARE)));
        }
        cx.notify();
    }

    /// Slice parity:chatlist-badge-settings: one labeled kit Switch row
    /// for the app badge counter section.
    pub(in crate::ui) fn badge_switch_row(
        &self,
        cx: &mut Context<Self>,
        id: &str,
        label: &str,
        checked: bool,
        apply: fn(&mut BadgePrefs, bool),
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(label.to_owned()),
            )
            .child(
                Switch::new(format!("badge-{id}"))
                    .checked(checked)
                    .accessibility_label(label)
                    .on_click(cx.listener(move |this, &on, _, cx| {
                        this.set_badge_pref(|p| apply(p, on), cx);
                    })),
            )
            .into_any_element()
    }

    /// Slice parity:chatlist-badge-settings: app badge counter
    /// preferences (include muted / include archived / messages vs
    /// chats) as kit Switch rows on the Notifications page.
    /// Toggling persists via `set_badge_pref`; the tray picks the new
    /// count up on its next 1s sync.
    pub(in crate::ui) fn badge_counter_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let prefs = self
            .session()
            .map(|s| s.settings.badge_prefs)
            .unwrap_or_default();
        div()
            .id("badge-counter-section")
            .flex()
            .flex_col()
            .gap_1()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .font_semibold()
                            .text_sm()
                            .child(tr(keys::Q_PREFS_APP_BADGE_COUNTER)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(keys::Q_PREFS_TRAY_ICON_BADGE)),
                    ),
            )
            .child(self.badge_switch_row(
                cx,
                "include-muted",
                tr(keys::Q_PREFS_INCLUDE_MUTED_CHATS),
                prefs.include_muted,
                |p, on| p.include_muted = on,
            ))
            .child(self.badge_switch_row(
                cx,
                "include-archived",
                tr(keys::Q_PREFS_INCLUDE_ARCHIVED_CHATS),
                prefs.include_archived,
                |p, on| p.include_archived = on,
            ))
            .child(self.badge_switch_row(
                cx,
                "include-muted-folders",
                tr(keys::SETTINGS_INCLUDE_MUTED_FOLDERS),
                prefs.include_muted_folders,
                |p, on| p.include_muted_folders = on,
            ))
            .child(self.badge_switch_row(
                cx,
                "count-messages",
                tr(keys::Q_PREFS_COUNT_UNREAD_MESSAGES_OFF_COUNT),
                prefs.count_messages,
                |p, on| p.count_messages = on,
            ))
            .into_any_element()
    }

    /// tdesktop "Bounce the Dock icon" / "Flash the taskbar icon" / "Draw
    /// attention to the window" (`flashBounceNotify`), stored with the other
    /// per-account counter preferences.
    pub(in crate::ui) fn attention_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self
            .session()
            .map(|s| s.settings.badge_prefs)
            .unwrap_or_default()
            .flash_bounce;
        let label = quill::notify_focus::attention_label();
        div()
            .id("attention-section")
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .child(div().font_semibold().text_sm().child(label))
            .child(
                Switch::new("attention-toggle")
                    .checked(on)
                    .accessibility_label(label)
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.set_badge_pref(|p| p.flash_bounce = on, cx);
                    })),
            )
            .into_any_element()
    }

    /// tdesktop "Events": "Contact joined Telegram" is the account-wide
    /// TDLib option `disable_contact_registered_notifications`. Pinned
    /// messages follow the per-chat-type switches above (TDLib keeps that
    /// choice per scope, not once for the app).
    pub(in crate::ui) fn events_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self
            .session()
            .is_none_or(|s| !s.settings.disable_contact_registered_notifications);
        div()
            .id("events-section")
            .flex()
            .flex_col()
            .gap_1()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .font_semibold()
                    .text_sm()
                    .child(tr(keys::SETTINGS_EVENTS_TITLE)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(keys::SETTINGS_EVENTS_JOINED)),
                    )
                    .child(
                        Switch::new("contact-joined-toggle")
                            .checked(on)
                            .accessibility_label(tr(keys::SETTINGS_EVENTS_JOINED))
                            .on_click(cx.listener(|this, &on, _, cx| {
                                this.set_contact_joined_notifications(on, cx);
                            })),
                    ),
            )
            .into_any_element()
    }

    /// `setOption(disable_contact_registered_notifications)`. Live: TDLib
    /// answers with `updateOption`. Demo: flips the fixture.
    pub(in crate::ui) fn set_contact_joined_notifications(
        &mut self,
        on: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live.driver.set_contact_joined_notifications(on) {
                self.connection.set_note(StatusNote::error(user_error_for(
                    keys::Q_NOTES_FAILED_NOTIFICATION_SETTING,
                    &err,
                )));
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.settings.disable_contact_registered_notifications = !on;
        }
        cx.notify();
    }

    /// Parity slice: in-app notification sounds (tdesktop "Play sounds")
    /// as a single kit Switch row on the Notifications page.
    /// Toggling writes through to `prefs.json` and updates the Session
    /// mirror so the next notification's sound decision sees it.
    /// tdesktop "Desktop notifications" switch.
    pub(in crate::ui) fn desktop_notifications_section(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let enabled = self.desktop_notifications_enabled();
        div()
            .id("desktop-notifications-section")
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .font_semibold()
                    .text_sm()
                    .child(tr(keys::SETTINGS_DESKTOP_NOTIFY)),
            )
            .child(
                Switch::new("desktop-notifications-toggle")
                    .checked(enabled)
                    .accessibility_label(tr(keys::SETTINGS_DESKTOP_NOTIFY))
                    .on_click(cx.listener(move |this, &on, _, cx| {
                        this.set_desktop_notifications(on, cx);
                    })),
            )
            .into_any_element()
    }

    pub(in crate::ui) fn inapp_sounds_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let enabled = self
            .session()
            .map(|s| s.settings.inapp_sounds_enabled)
            .unwrap_or(true);
        div()
            .id("inapp-sounds-section")
            .flex()
            .flex_col()
            .gap_1()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .font_semibold()
                            .text_sm()
                            .child(tr(keys::Q_PREFS_IN_APP_SOUNDS)),
                    )
                    .child(
                        Switch::new("inapp-sounds-toggle")
                            .checked(enabled)
                            .accessibility_label(tr(keys::Q_PREFS_PLAY_SOUNDS))
                            .on_click(cx.listener(move |this, &on, _, cx| {
                                this.set_inapp_sounds_enabled(on, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_PREFS_PLAY_SOUNDS_WHILE_USING_THE)),
            )
            .into_any_element()
    }

    /// Parity slice: `resetAllNotificationSettings` from the defaults
    /// dialog's "Reset all" button — resets all chat and scope notification
    /// settings to the server defaults. Live: the driver sends it; the ok
    /// arm clears the cached scope defaults and the authoritative updates
    /// refill them. Demo: seed the schema defaults directly.
    pub(in crate::ui) fn reset_all_notification_settings(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection
                .set_note(match live.driver.reset_all_notification_settings() {
                    Ok(_) => {
                        StatusNote::silent(tr(keys::Q_PREFS_RESETTING_ALL_NOTIFICATION_SETTINGS))
                    }
                    Err(_) => {
                        StatusNote::error(tr(keys::Q_PREFS_COULD_NOT_RESET_NOTIFICATION_SETTINGS))
                    }
                });
        } else if let Some(session) = self.demo_session.as_mut() {
            for scope in NotificationSettingsScope::ALL {
                session
                    .settings
                    .scope_notification_settings
                    .insert(scope, ScopeNotificationSettings::default());
            }
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_PREFS_NOTIFICATION_SETTINGS_RESET,
            )));
        }
        cx.notify();
    }

    /// Parity slice: the "Reset all" confirmation banner on the
    /// Notifications page — the `sessions_confirm_banner` pattern. Destructive
    /// `resetAllNotificationSettings` has no undo, so the page's "Reset
    /// all" row arms this banner instead of sending the request.
    pub(in crate::ui) fn notifications_confirm_banner(
        &self,
        confirm: NotificationsConfirm,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let question = match confirm {
            NotificationsConfirm::ResetAll => tr(keys::Q_PREFS_RESET_ALL_NOTIFICATION_SETTINGS_TO),
        };
        div()
            .id("notifications-confirm")
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(danger())
            .bg(danger_bg())
            .child(
                div()
                    .text_sm()
                    .font_medium()
                    .text_color(danger())
                    .child(question),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("notifications-confirm-cancel")
                            .small()
                            .label(tr(keys::CANCEL))
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cancel_notifications_reset(cx);
                            })),
                    )
                    .child(
                        Button::new("notifications-confirm-reset")
                            .small()
                            .label(tr(keys::Q_PREFS_RESET))
                            .danger()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirm_notifications_reset(cx);
                            })),
                    ),
            )
    }

    /// Parity slice: drop the pending "Reset all" confirmation.
    pub(in crate::ui) fn cancel_notifications_reset(&mut self, cx: &mut Context<Self>) {
        self.notify.notifications_confirm = None;
        cx.notify();
    }

    /// Parity slice: send the confirmed reset. Live: the driver sends
    /// `resetAllNotificationSettings`; demo: seed the schema defaults
    /// directly — both via the shared `reset_all_notification_settings`.
    pub(in crate::ui) fn confirm_notifications_reset(&mut self, cx: &mut Context<Self>) {
        self.notify.notifications_confirm.take();
        self.reset_all_notification_settings(cx);
    }
}
