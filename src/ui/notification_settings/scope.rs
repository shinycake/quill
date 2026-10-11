//! Scope defaults (private chats, groups, channels) and their
//! exceptions.

use super::*;

impl QuillApp {
    /// Parity slice: apply a scope's sound default
    /// (`setScopeNotificationSettings`).
    pub(in crate::ui) fn apply_scope_sound(
        &mut self,
        scope: NotificationSettingsScope,
        sound_id: i64,
        cx: &mut Context<Self>,
    ) {
        self.notify.defaults_sound_picker = None;
        if let Some(live) = self.live.as_mut() {
            // Guard: never send schema-defaults as current state — if the
            // scope's settings haven't arrived yet, wait for the fetch
            // instead (the dialog already shows "Loading…" per scope).
            let Some(mut settings) = live
                .driver
                .session
                .settings
                .scope_notification_settings
                .get(&scope)
                .cloned()
            else {
                self.connection.status_note = "defaults still loading…".into();
                cx.notify();
                return;
            };
            settings.sound_id = sound_id;
            let result = live
                .driver
                .send_scope_notification_settings(scope, &settings);
            self.connection.status_note = match result {
                Ok(_) => "default sound updated…".into(),
                Err(_) => "could not change default sound".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            // Screenshot demo: apply locally so the dialog reflects it.
            let mut settings = session
                .settings
                .scope_notification_settings
                .get(&scope)
                .cloned()
                .unwrap_or_default();
            settings.sound_id = sound_id;
            session
                .settings
                .scope_notification_settings
                .insert(scope, settings);
            self.connection.status_note = "default sound updated".into();
        }
        cx.notify();
    }

    /// Parity slice: apply a scope's mute default.
    pub(in crate::ui) fn apply_scope_mute(
        &mut self,
        scope: NotificationSettingsScope,
        mute_for: i32,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            // Guard: never send schema-defaults as current state — if the
            // scope's settings haven't arrived yet, wait for the fetch
            // instead (the dialog already shows "Loading…" per scope).
            let Some(mut settings) = live
                .driver
                .session
                .settings
                .scope_notification_settings
                .get(&scope)
                .cloned()
            else {
                self.connection.status_note = "defaults still loading…".into();
                cx.notify();
                return;
            };
            settings.mute_for = mute_for;
            let result = live
                .driver
                .send_scope_notification_settings(scope, &settings);
            self.connection.status_note = match result {
                Ok(_) if mute_for == 0 => "default unmuted…".into(),
                Ok(_) => "default mute updated…".into(),
                Err(_) => "could not change default mute".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            let mut settings = session
                .settings
                .scope_notification_settings
                .get(&scope)
                .cloned()
                .unwrap_or_default();
            settings.mute_for = mute_for;
            session
                .settings
                .scope_notification_settings
                .insert(scope, settings);
            self.connection.status_note = "default mute updated".into();
        }
        cx.notify();
    }

    /// Parity slice: apply a scope's preview default.
    pub(in crate::ui) fn apply_scope_preview(
        &mut self,
        scope: NotificationSettingsScope,
        show_preview: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            // Guard: never send schema-defaults as current state — if the
            // scope's settings haven't arrived yet, wait for the fetch
            // instead (the dialog already shows "Loading…" per scope).
            let Some(mut settings) = live
                .driver
                .session
                .settings
                .scope_notification_settings
                .get(&scope)
                .cloned()
            else {
                self.connection.status_note = "defaults still loading…".into();
                cx.notify();
                return;
            };
            settings.show_preview = show_preview;
            let result = live
                .driver
                .send_scope_notification_settings(scope, &settings);
            self.connection.status_note = match result {
                Ok(_) => "default preview updated…".into(),
                Err(_) => "could not change default preview".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            let mut settings = session
                .settings
                .scope_notification_settings
                .get(&scope)
                .cloned()
                .unwrap_or_default();
            settings.show_preview = show_preview;
            session
                .settings
                .scope_notification_settings
                .insert(scope, settings);
            self.connection.status_note = "default preview updated".into();
        }
        cx.notify();
    }

    /// Parity slice: apply a scope's story-mute default
    /// (`setScopeNotificationSettings`).
    pub(in crate::ui) fn apply_scope_story_mute(
        &mut self,
        scope: NotificationSettingsScope,
        mute_stories: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            // Guard: never send schema-defaults as current state — if the
            // scope's settings haven't arrived yet, wait for the fetch
            // instead (the dialog already shows "Loading…" per scope).
            let Some(mut settings) = live
                .driver
                .session
                .settings
                .scope_notification_settings
                .get(&scope)
                .cloned()
            else {
                self.connection.status_note = "defaults still loading…".into();
                cx.notify();
                return;
            };
            settings.mute_stories = mute_stories;
            let result = live
                .driver
                .send_scope_notification_settings(scope, &settings);
            self.connection.status_note = match result {
                Ok(_) => "story notification default updated…".into(),
                Err(_) => "could not change story notification default".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            let mut settings = session
                .settings
                .scope_notification_settings
                .get(&scope)
                .cloned()
                .unwrap_or_default();
            settings.mute_stories = mute_stories;
            session
                .settings
                .scope_notification_settings
                .insert(scope, settings);
            self.connection.status_note = "story notification default updated".into();
        }
        cx.notify();
    }

    /// Parity slice: apply a scope's mention-notification override
    /// (`setScopeNotificationSettings`).
    pub(in crate::ui) fn apply_scope_mention_notif(
        &mut self,
        scope: NotificationSettingsScope,
        notify: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            // Guard: never send schema-defaults as current state — if the
            // scope's settings haven't arrived yet, wait for the fetch
            // instead (the dialog already shows "Loading…" per scope).
            let Some(mut settings) = live
                .driver
                .session
                .settings
                .scope_notification_settings
                .get(&scope)
                .cloned()
            else {
                self.connection.status_note = "defaults still loading…".into();
                cx.notify();
                return;
            };
            settings.disable_mention_notifications = !notify;
            let result = live
                .driver
                .send_scope_notification_settings(scope, &settings);
            self.connection.status_note = match result {
                Ok(_) => "default mention notifications updated…".into(),
                Err(_) => "could not change default mention notifications".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            let mut settings = session
                .settings
                .scope_notification_settings
                .get(&scope)
                .cloned()
                .unwrap_or_default();
            settings.disable_mention_notifications = !notify;
            session
                .settings
                .scope_notification_settings
                .insert(scope, settings);
            self.connection.status_note = "default mention notifications updated".into();
        }
        cx.notify();
    }

    /// Parity slice: apply a scope's story-poster default
    /// (`setScopeNotificationSettings`).
    pub(in crate::ui) fn apply_scope_story_poster(
        &mut self,
        scope: NotificationSettingsScope,
        show_story_poster: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            // Guard: never send schema-defaults as current state — if the
            // scope's settings haven't arrived yet, wait for the fetch
            // instead (the dialog already shows "Loading…" per scope).
            let Some(mut settings) = live
                .driver
                .session
                .settings
                .scope_notification_settings
                .get(&scope)
                .cloned()
            else {
                self.connection.status_note = "defaults still loading…".into();
                cx.notify();
                return;
            };
            settings.show_story_poster = show_story_poster;
            let result = live
                .driver
                .send_scope_notification_settings(scope, &settings);
            self.connection.status_note = match result {
                Ok(_) => "story poster default updated…".into(),
                Err(_) => "could not change story poster default".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            let mut settings = session
                .settings
                .scope_notification_settings
                .get(&scope)
                .cloned()
                .unwrap_or_default();
            settings.show_story_poster = show_story_poster;
            session
                .settings
                .scope_notification_settings
                .insert(scope, settings);
            self.connection.status_note = "story poster default updated".into();
        }
        cx.notify();
    }

    /// Parity slice: apply a scope's pinned-message-notification override
    /// (`setScopeNotificationSettings`).
    pub(in crate::ui) fn apply_scope_pinned_notif(
        &mut self,
        scope: NotificationSettingsScope,
        notify: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            // Guard: never send schema-defaults as current state — if the
            // scope's settings haven't arrived yet, wait for the fetch
            // instead (the dialog already shows "Loading…" per scope).
            let Some(mut settings) = live
                .driver
                .session
                .settings
                .scope_notification_settings
                .get(&scope)
                .cloned()
            else {
                self.connection.status_note = "defaults still loading…".into();
                cx.notify();
                return;
            };
            settings.disable_pinned_message_notifications = !notify;
            let result = live
                .driver
                .send_scope_notification_settings(scope, &settings);
            self.connection.status_note = match result {
                Ok(_) => "default pinned-message notifications updated…".into(),
                Err(_) => "could not change default pinned-message notifications".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            let mut settings = session
                .settings
                .scope_notification_settings
                .get(&scope)
                .cloned()
                .unwrap_or_default();
            settings.disable_pinned_message_notifications = !notify;
            session
                .settings
                .scope_notification_settings
                .insert(scope, settings);
            self.connection.status_note = "default pinned-message notifications updated".into();
        }
        cx.notify();
    }

    /// Parity slice: one scope's section in the defaults dialog.
    pub(in crate::ui) fn scope_settings_section(
        &self,
        cx: &mut Context<Self>,
        scope: NotificationSettingsScope,
        saved_sounds: &[NotificationSound],
    ) -> AnyElement {
        let settings: ScopeNotificationSettings = self
            .session()
            .and_then(|s| s.settings.scope_notification_settings.get(&scope).cloned())
            .unwrap_or_default();
        let loaded = self
            .session()
            .is_some_and(|s| s.settings.scope_notification_settings.contains_key(&scope));
        let muted = settings.mute_for > 0;
        let sound_label = match settings.sound_id {
            -1 => "Default".to_string(),
            0 => "None".to_string(),
            id => saved_sounds
                .iter()
                .find(|s| s.id == id)
                .map(|s| s.title.clone())
                .unwrap_or_else(|| "Custom".to_string()),
        };
        let current_choice = match settings.sound_id {
            -1 => SoundChoice::Default,
            0 => SoundChoice::Disabled,
            id => SoundChoice::Custom(id),
        };

        let presets = [
            ("Unmute", 0),
            ("1 hour", MUTE_FOR_1_HOUR),
            ("8 hours", MUTE_FOR_8_HOURS),
            ("2 days", MUTE_FOR_2_DAYS),
            ("Forever", MUTE_FOREVER),
        ];
        let mut preset_row = div().flex().flex_wrap().gap_1();
        for (label, seconds) in presets {
            preset_row = preset_row.child(
                Button::new(format!("scope-mute-{scope:?}-{seconds}"))
                    .small()
                    .label(label)
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.apply_scope_mute(scope, seconds, cx);
                    })),
            );
        }

        let mut section = div()
            .id(format!("scope-section-{:?}", scope))
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
                    .child(div().font_semibold().text_sm().child(scope.label()))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(if loaded {
                                if muted { "Muted" } else { "Not muted" }.to_string()
                            } else {
                                "Loading…".to_string()
                            }),
                    ),
            )
            .child(preset_row)
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
                            .child("Show message preview"),
                    )
                    .child(
                        // Phase 6: kit Switch (was: On/Off ghost button).
                        Switch::new(format!("scope-preview-{scope:?}"))
                            .checked(settings.show_preview)
                            .accessibility_label("Show message preview")
                            .on_click(cx.listener(move |this, &on, _, cx| {
                                this.apply_scope_preview(scope, on, cx);
                            })),
                    ),
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
                            .child("Notify about mentions"),
                    )
                    .child(
                        // Parity slice: mention-notification override per scope
                        // (schema: disable_mention_notifications).
                        Switch::new(format!("scope-mentions-{scope:?}"))
                            .checked(!settings.disable_mention_notifications)
                            .accessibility_label("Notify about mentions")
                            .on_click(cx.listener(move |this, &on, _, cx| {
                                this.apply_scope_mention_notif(scope, on, cx);
                            })),
                    ),
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
                            .child("Notify about pinned messages"),
                    )
                    .child(
                        // Parity slice: pinned-message-notification override
                        // per scope (schema: disable_pinned_message_notifications).
                        Switch::new(format!("scope-pinned-{scope:?}"))
                            .checked(!settings.disable_pinned_message_notifications)
                            .accessibility_label("Notify about pinned messages")
                            .on_click(cx.listener(move |this, &on, _, cx| {
                                this.apply_scope_pinned_notif(scope, on, cx);
                            })),
                    ),
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
                            .child(format!("Sound: {sound_label}")),
                    )
                    .child(
                        Button::new(format!("scope-sound-{:?}", scope))
                            .small()
                            .label(
                                if self.notify.defaults_sound_picker
                                    == Some(SoundPickerTarget::Scope(scope))
                                {
                                    "Hide"
                                } else {
                                    "Change"
                                },
                            )
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let target = SoundPickerTarget::Scope(scope);
                                this.notify.defaults_sound_picker =
                                    if this.notify.defaults_sound_picker == Some(target) {
                                        None
                                    } else {
                                        Some(target)
                                    };
                                cx.notify();
                            })),
                    ),
            );
        if self.notify.defaults_sound_picker == Some(SoundPickerTarget::Scope(scope)) {
            section = section.child(self.notification_sound_picker(
                cx,
                SoundPickerTarget::Scope(scope),
                current_choice,
                saved_sounds,
            ));
        }
        section = section
            .child(
                // Parity slice: story fields of `scopeNotificationSettings`
                // (schema line 3369-3375) — no story-sound picker (the
                // message-sound picker above already covers the per-scope
                // sound choice).
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Mute story notifications"),
                    )
                    .child(
                        Switch::new(format!("scope-story-mute-{scope:?}"))
                            .checked(settings.mute_stories)
                            .accessibility_label("Mute story notifications")
                            .on_click(cx.listener(move |this, &on, _, cx| {
                                this.apply_scope_story_mute(scope, on, cx);
                            })),
                    ),
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
                            .child("Show story poster"),
                    )
                    .child(
                        Switch::new(format!("scope-story-poster-{scope:?}"))
                            .checked(settings.show_story_poster)
                            .accessibility_label("Show story poster")
                            .on_click(cx.listener(move |this, &on, _, cx| {
                                this.apply_scope_story_poster(scope, on, cx);
                            })),
                    ),
            );
        // Parity slice: per-scope notification exceptions
        // (`getChatNotificationSettingsExceptions`) — fetched on dialog
        // open; "Loading…" while the request is in flight.
        let exceptions = self
            .session()
            .and_then(|s| s.settings.notification_exceptions.get(&scope));
        let exceptions_loading = self
            .session()
            .is_some_and(|s| s.settings.notification_exceptions_loading.contains(&scope));
        let exceptions_label = match (exceptions, exceptions_loading) {
            (Some(ids), _) => {
                if ids.len() == 1 {
                    "1 chat".to_string()
                } else {
                    format!("{} chats", ids.len())
                }
            }
            (None, true) => "Loading…".to_string(),
            (None, false) => "—".to_string(),
        };
        section = section.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("Exceptions: {exceptions_label}")),
                )
                .child(
                    Button::new(format!("scope-exceptions-{scope:?}"))
                        .small()
                        .label(if self.notify.defaults_exceptions_scope == Some(scope) {
                            "Hide"
                        } else {
                            "View"
                        })
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.notify.defaults_exceptions_scope =
                                if this.notify.defaults_exceptions_scope == Some(scope) {
                                    None
                                } else {
                                    Some(scope)
                                };
                            cx.notify();
                        })),
                ),
        );
        if self.notify.defaults_exceptions_scope == Some(scope) {
            section = section.child(self.exceptions_list(cx, scope));
        }
        section.into_any_element()
    }

    /// Parity slice: the expanded exceptions sub-view for one scope — the
    /// exception chats (titles resolved from the session chats map) with a
    /// "Reset to default" button per chat.
    pub(in crate::ui) fn exceptions_list(
        &self,
        cx: &mut Context<Self>,
        scope: NotificationSettingsScope,
    ) -> AnyElement {
        let session = self.session();
        let cached: Option<&Vec<i64>> = session
            .as_ref()
            .and_then(|s| s.settings.notification_exceptions.get(&scope));
        let loading = session
            .as_ref()
            .is_some_and(|s| s.settings.notification_exceptions_loading.contains(&scope));
        let mut list = div().flex().flex_col().gap_1().px_2().py_1();
        let Some(ids) = cached else {
            // Fetch failed or never fired — the next dialog open retries.
            return list
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(if loading {
                            "Loading…"
                        } else {
                            "Exceptions unavailable — reopen the dialog to retry."
                        }),
                )
                .into_any_element();
        };
        if ids.is_empty() {
            list = list.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("No exceptions — every chat uses the scope default."),
            );
        }
        for &id in ids {
            let title = session
                .as_ref()
                .and_then(|s| s.chats.get(&id))
                .map(|c| c.title.clone())
                .unwrap_or_else(|| format!("Chat {id}"));
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .min_w_0()
                            .truncate()
                            .child(crate::ui::bidi_line::one_line_plain(title)),
                    )
                    .child(
                        Button::new(format!("exception-reset-{id}"))
                            .small()
                            .label("Reset to default")
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.reset_notification_exception(ChatId(id), cx);
                            })),
                    ),
            );
        }
        list.into_any_element()
    }

    /// Parity slice: reset one exception chat to the scope defaults
    /// (`setChatNotificationSettings` with every `use_default_*` flag set).
    pub(in crate::ui) fn reset_notification_exception(
        &mut self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            let result = live.driver.reset_chat_notification_settings(chat_id);
            self.connection.status_note = match result {
                Ok(_) => "exception reset…".into(),
                Err(_) => "could not reset exception".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            // Screenshot demo: apply locally so the list reflects it.
            if let Some(chat) = session.chats.get_mut(&chat_id.0) {
                chat.notification_settings = ChatNotificationSettings::default();
            }
            for list in session.settings.notification_exceptions.values_mut() {
                list.retain(|id| *id != chat_id.0);
            }
            self.connection.status_note = "exception reset".into();
        }
        cx.notify();
    }
}
