//! App-wide notification settings: badge counter, desktop notifications,
//! in-app sounds, the defaults dialog sections, and Reset All.

use super::*;

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
                self.connection.status_note = format!("couldn’t save badge settings: {err}");
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.settings.badge_prefs = prefs;
            self.connection.status_note = "demo: badge settings are not saved".into();
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
            if let Err(err) = live.driver.save_desktop_notifications() {
                self.connection.status_note = format!("couldn’t save desktop notifications: {err}");
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.settings.desktop_notifications = on;
            self.connection.status_note = "demo: desktop notifications are not saved".into();
        }
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
            if let Err(err) = live.driver.save_inapp_sounds_enabled() {
                self.connection.status_note = format!("couldn’t save notification sounds: {err}");
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.settings.inapp_sounds_enabled = on;
            self.connection.status_note = "demo: in-app sounds are not saved".into();
        }
        cx.notify();
    }

    /// kit Phase 2 (redo): notification defaults hosted in a kit `Dialog`
    /// via `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(in crate::ui) fn build_notification_defaults_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close = QuillShell::on_close_kind(
            app,
            shell,
            DialogKind::NotificationDefaults,
            |this, _, cx| {
                this.notify.notification_defaults_open = false;
                this.notify.defaults_sound_picker = None;
                this.notify.defaults_exceptions_scope = None;
                this.notify.notifications_confirm = None;
                cx.notify();
            },
        );
        app.update(cx, |this, cx| {
            let session = this.session();
            let saved_sounds: Vec<NotificationSound> = session
                .as_ref()
                .map(|s| s.settings.saved_notification_sounds.clone())
                .unwrap_or_default();
            let mut body = div().flex().flex_col().gap_3();
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        "Used when a chat keeps the default setting. \
                         Changes apply via setScopeNotificationSettings.",
                    ),
            );
            if let Some(confirm) = this.notify.notifications_confirm {
                body = body.child(this.notifications_confirm_banner(confirm, cx));
            }
            for scope in NotificationSettingsScope::ALL {
                body = body.child(this.scope_settings_section(cx, scope, &saved_sounds));
            }
            // Parity slice: reaction + poll-vote notification settings
            // (`setReactionNotificationSettings`).
            body = body.child(this.reaction_settings_section(cx, &saved_sounds));
            // Slice parity:chatlist-badge-settings: app badge counter
            // preferences (include muted/archived, messages vs chats).
            body = body.child(this.badge_counter_section(cx));
            // Parity slice: in-app notification sounds (tdesktop "Play
            // sounds") — the client-side toggle gating
            // `Session::notification_sound_for`.
            body = body.child(this.desktop_notifications_section(cx));
            body = body.child(this.attention_section(cx));
            body = body.child(this.events_section(cx));
            body = body.child(this.inapp_sounds_section(cx));
            this.ensure_notification_volume_slider(cx);
            body = body.child(this.notification_volume_section(cx));
            let footer = div().flex().justify_end().gap_2().children([
                Button::new("reset-all-notif-settings")
                    .small()
                    .label("Reset all")
                    .danger()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.notify.notifications_confirm = Some(NotificationsConfirm::ResetAll);
                        cx.notify();
                    }))
                    .into_any_element(),
                Button::new("close-notif-defaults")
                    .small()
                    .label("Close")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.notify.notification_defaults_open = false;
                        this.notify.defaults_sound_picker = None;
                        this.notify.defaults_exceptions_scope = None;
                        this.notify.notifications_confirm = None;
                        cx.notify();
                        this.close_kit_dialog_if_done(DialogKind::NotificationDefaults, window, cx);
                    }))
                    .into_any_element(),
            ]);
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title("Notification defaults"))
                .content(crate::ui::shell::scrollable_dialog_content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
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
    /// chats) as kit Switch rows in the notification defaults dialog.
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
                    .child(div().font_semibold().text_sm().child("App badge counter"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Tray icon badge"),
                    ),
            )
            .child(self.badge_switch_row(
                cx,
                "include-muted",
                "Include muted chats",
                prefs.include_muted,
                |p, on| p.include_muted = on,
            ))
            .child(self.badge_switch_row(
                cx,
                "include-archived",
                "Include archived chats",
                prefs.include_archived,
                |p, on| p.include_archived = on,
            ))
            .child(self.badge_switch_row(
                cx,
                "include-muted-folders",
                "Include muted chats in folder counters",
                prefs.include_muted_folders,
                |p, on| p.include_muted_folders = on,
            ))
            .child(self.badge_switch_row(
                cx,
                "count-messages",
                "Count unread messages (off: count chats)",
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
            .child(div().font_semibold().text_sm().child("Events"))
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
                            .child("Contact joined Telegram"),
                    )
                    .child(
                        Switch::new("contact-joined-toggle")
                            .checked(on)
                            .accessibility_label("Contact joined Telegram")
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
                self.connection.status_note = format!("couldn’t change the setting: {err:?}");
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.settings.disable_contact_registered_notifications = !on;
        }
        cx.notify();
    }

    /// Parity slice: in-app notification sounds (tdesktop "Play sounds")
    /// as a single kit Switch row in the notification defaults dialog.
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
                    .child("Desktop notifications"),
            )
            .child(
                Switch::new("desktop-notifications-toggle")
                    .checked(enabled)
                    .accessibility_label("Desktop notifications")
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
                    .child(div().font_semibold().text_sm().child("In-app sounds"))
                    .child(
                        Switch::new("inapp-sounds-toggle")
                            .checked(enabled)
                            .accessibility_label("Play sounds")
                            .on_click(cx.listener(move |this, &on, _, cx| {
                                this.set_inapp_sounds_enabled(on, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Play sounds while using the app"),
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
            self.connection.status_note = match live.driver.reset_all_notification_settings() {
                Ok(_) => "resetting all notification settings…".into(),
                Err(_) => "could not reset notification settings".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            for scope in NotificationSettingsScope::ALL {
                session
                    .settings
                    .scope_notification_settings
                    .insert(scope, ScopeNotificationSettings::default());
            }
            self.connection.status_note = "notification settings reset".into();
        }
        cx.notify();
    }

    /// Parity slice: the "Reset all" confirmation banner in the defaults
    /// dialog — the `sessions_confirm_banner` pattern. Destructive
    /// `resetAllNotificationSettings` has no undo, so the footer's "Reset
    /// all" button arms this banner instead of sending the request.
    pub(in crate::ui) fn notifications_confirm_banner(
        &self,
        confirm: NotificationsConfirm,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let question = match confirm {
            NotificationsConfirm::ResetAll => {
                "Reset all notification settings to the server defaults? This cannot be undone."
            }
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
                            .label("Cancel")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cancel_notifications_reset(cx);
                            })),
                    )
                    .child(
                        Button::new("notifications-confirm-reset")
                            .small()
                            .label("Reset")
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
