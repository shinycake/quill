//! Scope defaults (private chats, groups, channels) and their
//! exceptions.

use super::*;
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr, tr_with, trn};

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
                self.connection
                    .set_note(StatusNote::silent(tr(keys::Q_PREFS_DEFAULTS_STILL_LOADING)));
                cx.notify();
                return;
            };
            settings.sound_id = sound_id;
            let result = live
                .driver
                .send_scope_notification_settings(scope, &settings);
            self.connection.set_note(match result {
                Ok(_) => StatusNote::silent(tr(keys::Q_PREFS_DEFAULT_SOUND_UPDATED)),
                Err(_) => StatusNote::error(tr(keys::Q_PREFS_COULD_NOT_CHANGE_DEFAULT_SOUND)),
            });
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
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_PREFS_DEFAULT_SOUND_UPDATED_2,
            )));
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
                self.connection
                    .set_note(StatusNote::silent(tr(keys::Q_PREFS_DEFAULTS_STILL_LOADING)));
                cx.notify();
                return;
            };
            settings.mute_for = mute_for;
            let result = live
                .driver
                .send_scope_notification_settings(scope, &settings);
            self.connection.set_note(match result {
                Ok(_) if mute_for == 0 => StatusNote::silent(tr(keys::Q_PREFS_DEFAULT_UNMUTED)),
                Ok(_) => StatusNote::silent(tr(keys::Q_PREFS_DEFAULT_MUTE_UPDATED)),
                Err(_) => StatusNote::error(tr(keys::Q_PREFS_COULD_NOT_CHANGE_DEFAULT_MUTE)),
            });
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
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_PREFS_DEFAULT_MUTE_UPDATED_2)));
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
                self.connection
                    .set_note(StatusNote::silent(tr(keys::Q_PREFS_DEFAULTS_STILL_LOADING)));
                cx.notify();
                return;
            };
            settings.show_preview = show_preview;
            let result = live
                .driver
                .send_scope_notification_settings(scope, &settings);
            self.connection.set_note(match result {
                Ok(_) => StatusNote::silent(tr(keys::Q_PREFS_DEFAULT_PREVIEW_UPDATED)),
                Err(_) => StatusNote::error(tr(keys::Q_PREFS_COULD_NOT_CHANGE_DEFAULT_PREVIEW)),
            });
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
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_PREFS_DEFAULT_PREVIEW_UPDATED_2,
            )));
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
                self.connection
                    .set_note(StatusNote::silent(tr(keys::Q_PREFS_DEFAULTS_STILL_LOADING)));
                cx.notify();
                return;
            };
            settings.mute_stories = mute_stories;
            let result = live
                .driver
                .send_scope_notification_settings(scope, &settings);
            self.connection.set_note(match result {
                Ok(_) => StatusNote::silent(tr(keys::Q_PREFS_STORY_NOTIFICATION_DEFAULT_UPDATED)),
                Err(_) => StatusNote::error(tr(keys::Q_PREFS_COULD_NOT_CHANGE_STORY_NOTIFICATION)),
            });
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
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_PREFS_STORY_NOTIFICATION_DEFAULT_UPDATED_2,
            )));
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
                self.connection
                    .set_note(StatusNote::silent(tr(keys::Q_PREFS_DEFAULTS_STILL_LOADING)));
                cx.notify();
                return;
            };
            settings.disable_mention_notifications = !notify;
            let result = live
                .driver
                .send_scope_notification_settings(scope, &settings);
            self.connection.set_note(match result {
                Ok(_) => {
                    StatusNote::silent(tr(keys::Q_PREFS_DEFAULT_MENTION_NOTIFICATIONS_UPDATED))
                }
                Err(_) => StatusNote::error(tr(keys::Q_PREFS_COULD_NOT_CHANGE_DEFAULT_MENTION)),
            });
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
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_PREFS_DEFAULT_MENTION_NOTIFICATIONS_UPDATED_2,
            )));
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
                self.connection
                    .set_note(StatusNote::silent(tr(keys::Q_PREFS_DEFAULTS_STILL_LOADING)));
                cx.notify();
                return;
            };
            settings.show_story_poster = show_story_poster;
            let result = live
                .driver
                .send_scope_notification_settings(scope, &settings);
            self.connection.set_note(match result {
                Ok(_) => StatusNote::silent(tr(keys::Q_PREFS_STORY_POSTER_DEFAULT_UPDATED)),
                Err(_) => StatusNote::error(tr(keys::Q_PREFS_COULD_NOT_CHANGE_STORY_POSTER)),
            });
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
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_PREFS_STORY_POSTER_DEFAULT_UPDATED_2,
            )));
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
                self.connection
                    .set_note(StatusNote::silent(tr(keys::Q_PREFS_DEFAULTS_STILL_LOADING)));
                cx.notify();
                return;
            };
            settings.disable_pinned_message_notifications = !notify;
            let result = live
                .driver
                .send_scope_notification_settings(scope, &settings);
            self.connection.set_note(match result {
                Ok(_) => StatusNote::silent(tr(
                    keys::Q_PREFS_DEFAULT_PINNED_MESSAGE_NOTIFICATIONS_UPDATED,
                )),
                Err(_) => StatusNote::error(tr(keys::Q_PREFS_COULD_NOT_CHANGE_DEFAULT_PINNED)),
            });
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
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_PREFS_DEFAULT_PINNED_MESSAGE_NOTIFICATIONS_UPDATED_2,
            )));
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
            -1 => tr(keys::SETTINGS_NOTIFICATIONS_DISPLAY_DEFAULT).to_string(),
            0 => tr(keys::SETTINGS_NO_BLOCKED_USERS).to_string(),
            id => saved_sounds
                .iter()
                .find(|s| s.id == id)
                .map(|s| s.title.clone())
                .unwrap_or_else(|| tr(keys::Q_PREFS_CUSTOM_2).to_string()),
        };
        let current_choice = match settings.sound_id {
            -1 => SoundChoice::Default,
            0 => SoundChoice::Disabled,
            id => SoundChoice::Custom(id),
        };

        let presets = [
            (tr(keys::SETTINGS_QUICK_DIALOG_ACTION_UNMUTE).to_string(), 0),
            (trn(keys::HOURS, 1), MUTE_FOR_1_HOUR),
            (trn(keys::HOURS, 8), MUTE_FOR_8_HOURS),
            (trn(keys::DAYS, 2), MUTE_FOR_2_DAYS),
            (tr(keys::Q_PREFS_FOREVER).to_string(), MUTE_FOREVER),
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
                                if muted {
                                    tr(keys::NOTIFICATION_EXCEPTIONS_MUTED)
                                } else {
                                    tr(keys::Q_PREFS_NOT_MUTED)
                                }
                                .to_string()
                            } else {
                                tr(keys::Q_PREFS_LOADING).to_string()
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
                            .child(tr(keys::Q_PREFS_SHOW_MESSAGE_PREVIEW)),
                    )
                    .child(
                        // Phase 6: kit Switch (was: On/Off ghost button).
                        Switch::new(format!("scope-preview-{scope:?}"))
                            .checked(settings.show_preview)
                            .accessibility_label(tr(keys::Q_PREFS_SHOW_MESSAGE_PREVIEW))
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
                            .child(tr(keys::Q_PREFS_NOTIFY_ABOUT_MENTIONS)),
                    )
                    .child(
                        // Parity slice: mention-notification override per scope
                        // (schema: disable_mention_notifications).
                        Switch::new(format!("scope-mentions-{scope:?}"))
                            .checked(!settings.disable_mention_notifications)
                            .accessibility_label(tr(keys::Q_PREFS_NOTIFY_ABOUT_MENTIONS))
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
                            .child(tr(keys::Q_PREFS_NOTIFY_ABOUT_PINNED_MESSAGES)),
                    )
                    .child(
                        // Parity slice: pinned-message-notification override
                        // per scope (schema: disable_pinned_message_notifications).
                        Switch::new(format!("scope-pinned-{scope:?}"))
                            .checked(!settings.disable_pinned_message_notifications)
                            .accessibility_label(tr(keys::Q_PREFS_NOTIFY_ABOUT_PINNED_MESSAGES))
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
                            .child(tr_with(
                                keys::Q_PREFS_SOUND,
                                &[("sound_label", &sound_label.to_string())],
                            )),
                    )
                    .child(
                        Button::new(format!("scope-sound-{:?}", scope))
                            .small()
                            .label(
                                if self.notify.defaults_sound_picker
                                    == Some(SoundPickerTarget::Scope(scope))
                                {
                                    tr(keys::Q_PREFS_HIDE)
                                } else {
                                    tr(keys::Q_PREFS_CHANGE)
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
        if loaded {
            section = section.children(self.scope_volume_row(scope, settings.sound_id, cx));
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
                            .child(tr(keys::Q_PREFS_MUTE_STORY_NOTIFICATIONS)),
                    )
                    .child(
                        Switch::new(format!("scope-story-mute-{scope:?}"))
                            .checked(settings.mute_stories)
                            .accessibility_label(tr(keys::Q_PREFS_MUTE_STORY_NOTIFICATIONS))
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
                            .child(tr(keys::Q_PREFS_SHOW_STORY_POSTER)),
                    )
                    .child(
                        Switch::new(format!("scope-story-poster-{scope:?}"))
                            .checked(settings.show_story_poster)
                            .accessibility_label(tr(keys::Q_PREFS_SHOW_STORY_POSTER))
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
            (Some(ids), _) => trn(keys::FILTERS_CHATS_COUNT, ids.len()),
            (None, true) => tr(keys::Q_PREFS_LOADING).to_string(),
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
                        .child(tr_with(
                            keys::Q_PREFS_EXCEPTIONS,
                            &[("exceptions_label", &exceptions_label.to_string())],
                        )),
                )
                .child(
                    Button::new(format!("scope-exceptions-{scope:?}"))
                        .small()
                        .label(if self.notify.defaults_exceptions_scope == Some(scope) {
                            tr(keys::Q_PREFS_HIDE)
                        } else {
                            tr(keys::Q_PREFS_VIEW)
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
                            tr(keys::Q_PREFS_LOADING)
                        } else {
                            tr(keys::Q_PREFS_EXCEPTIONS_UNAVAILABLE_REOPEN_THE_DIALOG)
                        }),
                )
                .into_any_element();
        };
        if ids.is_empty() {
            list = list.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_PREFS_NO_EXCEPTIONS_EVERY_CHAT_USES)),
            );
        }
        for &id in ids {
            let title = session
                .as_ref()
                .and_then(|s| s.chats.get(&id))
                .map(|c| c.title.clone())
                .unwrap_or_else(|| tr_with(keys::Q_PREFS_CHAT, &[("id", &id.to_string())]));
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
                            .label(tr(keys::Q_PREFS_RESET_TO_DEFAULT))
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
            self.connection.set_note(match result {
                Ok(_) => StatusNote::silent(tr(keys::Q_PREFS_EXCEPTION_RESET)),
                Err(_) => StatusNote::error(tr(keys::Q_PREFS_COULD_NOT_RESET_EXCEPTION)),
            });
        } else if let Some(session) = self.demo_session.as_mut() {
            // Screenshot demo: apply locally so the list reflects it.
            if let Some(chat) = session.chats.get_mut(&chat_id.0) {
                chat.notification_settings = ChatNotificationSettings::default();
            }
            for list in session.settings.notification_exceptions.values_mut() {
                list.retain(|id| *id != chat_id.0);
            }
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_PREFS_EXCEPTION_RESET_2)));
        }
        cx.notify();
    }
}
