//! The per-chat mute menu.

use super::*;

impl QuillApp {
    pub(in crate::ui) fn close_mute_menu(&mut self, cx: &mut Context<Self>) {
        self.notify.mute_menu_open = false;
        self.notify.mute_custom_open = false;
        self.notify.notif_sound_picker_open = false;
        cx.notify();
    }

    pub(in crate::ui) fn open_mute_menu(&mut self, cx: &mut Context<Self>) {
        self.notify.mute_menu_open = true;
        self.notify.mute_custom_open = false;
        self.connection.status_note = "mute for…".into();
        cx.notify();
    }

    pub(in crate::ui) fn apply_chat_mute(
        &mut self,
        chat_id: ChatId,
        mute_for: i32,
        cx: &mut Context<Self>,
    ) {
        self.notify.mute_menu_open = false;
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .set_chat_mute_for(chat_id, mute_for);
            self.connection.status_note = match result {
                Ok(_) if mute_for == 0 => "unmuting…".into(),
                Ok(_) => "muting…".into(),
                Err(_) => "could not change mute".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_notification_settings(
                chat_id,
                |settings| {
                    *settings = settings.clone().with_mute_for(mute_for);
                },
                cx,
            );
            self.connection.status_note = if mute_for == 0 {
                "unmuted".into()
            } else {
                "muted".into()
            };
            cx.notify();
        }
    }

    /// Parity slice: the tdesktop "Mute" submenu is now a per-chat
    /// notification settings panel — mute presets, message-preview toggle,
    /// notification-sound picker (`getSavedNotificationSounds`), and a link
    /// to the scope defaults dialog. Current state shows in the summary line.
    ///
    /// Phase S1: secret chats additionally show the TGX
    /// `NotificationChannelSecretChat` label with the peer's name.
    pub(in crate::ui) fn mute_menu_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let open_chat = session.as_ref().and_then(|s| s.open_chat);
        let open_chat_summary: Option<&ChatSummary> =
            open_chat.and_then(|id| session.as_ref()?.chats.get(&id.0));
        let chat_settings: ChatNotificationSettings = open_chat_summary
            .map(|chat| chat.notification_settings.clone())
            .unwrap_or_default();
        let muted = open_chat_summary
            .and_then(|chat| session.as_ref().map(|s| s.effective_muted(chat)))
            .unwrap_or_else(|| chat_settings.is_muted());
        let preview_on = open_chat_summary
            .and_then(|chat| session.as_ref().map(|s| s.effective_preview_allowed(chat)))
            .unwrap_or(chat_settings.use_default_show_preview || chat_settings.show_preview);
        let sound_label = self.notification_sound_label(&chat_settings);
        // Parity slice (`parity:stories-notify-settings`): effective
        // per-chat story settings, falling back to the chat's own flags
        // when the chat is unknown to the session.
        let story_muted = open_chat_summary
            .and_then(|chat| session.as_ref().map(|s| s.effective_story_muted(chat)))
            .unwrap_or(chat_settings.mute_stories);
        let story_poster_on = open_chat_summary
            .and_then(|chat| session.as_ref().map(|s| s.effective_story_poster(chat)))
            .unwrap_or(chat_settings.show_story_poster);
        let story_sound_label = self.story_sound_label(&chat_settings);
        let saved_sounds: Vec<NotificationSound> = session
            .as_ref()
            .map(|s| s.saved_notification_sounds.clone())
            .unwrap_or_default();
        let status = format!(
            "{} \u{b7} Sound: {} \u{b7} Previews: {}",
            if muted {
                if chat_settings.is_muted_forever() {
                    "Muted forever"
                } else {
                    "Muted"
                }
            } else {
                "Unmuted"
            },
            sound_label,
            if preview_on { "on" } else { "off" },
        );

        let presets = [
            ("1 hour", MUTE_FOR_1_HOUR),
            ("8 hours", MUTE_FOR_8_HOURS),
            ("2 days", MUTE_FOR_2_DAYS),
            ("Forever", MUTE_FOREVER),
        ];
        let sound_disabled = !chat_settings.use_default_sound && chat_settings.sound_id == 0;
        let custom_open = self.notify.mute_custom_open;
        let custom = self.notify.mute_custom;
        let mut preset_row = div().id("mute-presets").flex().flex_wrap().gap_1();
        for (label, seconds) in presets {
            preset_row = preset_row.child(
                Button::new(format!("mute-for-{seconds}"))
                    .small()
                    .label(label)
                    .outline()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(chat_id) = open_chat {
                            this.apply_chat_mute(chat_id, seconds, cx);
                        }
                    })),
            );
        }

        // tdesktop `menu_mute.cpp`: after the presets come "Custom...",
        // and "Unmute" while the chat is muted.
        preset_row = preset_row.child(
            Button::new("mute-custom-toggle")
                .small()
                .label("Custom\u{2026}")
                .outline()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.notify.mute_custom_open = !this.notify.mute_custom_open;
                    cx.notify();
                })),
        );
        if muted {
            preset_row = preset_row.child(
                Button::new("mute-unmute")
                    .small()
                    .label("Unmute")
                    .outline()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(chat_id) = open_chat {
                            this.apply_chat_mute(chat_id, 0, cx);
                        }
                    })),
            );
        }
        let custom_row = custom_open.then(|| {
            let stepper = |id: &'static str, label: String, minus: bool, hours: bool| {
                let delta = if minus { -1 } else { 1 };
                Button::new(id)
                    .small()
                    .ghost()
                    .label(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.notify.mute_custom = if hours {
                            this.notify.mute_custom.step_hours(delta)
                        } else {
                            this.notify.mute_custom.step_days(delta)
                        };
                        cx.notify();
                    }))
            };
            div()
                .id("mute-custom")
                .flex()
                .flex_wrap()
                .items_center()
                .gap_1()
                .child(stepper(
                    "mute-days-minus",
                    "\u{2212} day".into(),
                    true,
                    false,
                ))
                .child(stepper("mute-days-plus", "+ day".into(), false, false))
                .child(stepper(
                    "mute-hours-minus",
                    "\u{2212} hour".into(),
                    true,
                    true,
                ))
                .child(stepper("mute-hours-plus", "+ hour".into(), false, true))
                .child(
                    Button::new("mute-custom-apply")
                        .small()
                        .label(format!("Mute for {}", custom.label()))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(chat_id) = open_chat {
                                let mute_for = this.notify.mute_custom.mute_for();
                                this.notify.mute_custom_open = false;
                                this.apply_chat_mute(chat_id, mute_for, cx);
                            }
                        })),
                )
        });
        let sound_toggle = Button::new("mute-sound-toggle")
            .small()
            .label(quill::mute_menu::sound_toggle_label(sound_disabled))
            .outline()
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(chat_id) = open_chat {
                    let (use_default, sound_id) =
                        quill::mute_menu::sound_toggle_target(sound_disabled);
                    this.apply_chat_sound(chat_id, use_default, sound_id, cx);
                }
            }));

        let mut panel = div()
            .id("mute-menu")
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_semibold().child("Notifications"))
                    .child(
                        Button::new("close-mute-menu")
                            .icon(gpui_kit::assets::IconName::X)
                            .tooltip("Close")
                            .accessibility_label("Close")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_mute_menu(cx);
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(status),
            )
            // Phase S1: TGX `NotificationChannelSecretChat` — secret chats
            // get their own custom-settings label with the peer's name.
            .when_some(Self::secret_notif_label(session), |this, label| {
                this.child(div().text_xs().text_color(cx.theme().accent).child(label))
            })
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground)
                    .child("Mute for"),
            )
            .child(preset_row)
            .children(custom_row)
            .child(sound_toggle)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .child("Show message preview in notifications"),
                    )
                    .child(
                        // Phase 6: kit Switch (was: On/Off ghost button).
                        Switch::new("notif-preview-toggle")
                            .checked(preview_on)
                            .accessibility_label("Show message preview in notifications")
                            .on_click(cx.listener(move |this, &on, _, cx| {
                                if let Some(chat_id) = open_chat {
                                    this.apply_chat_preview(chat_id, on, cx);
                                }
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
                            .text_sm()
                            .child(format!("Notification sound: {sound_label}")),
                    )
                    .child(
                        Button::new("notif-sound-picker-toggle")
                            .small()
                            .label(if self.notify.notif_sound_picker_open {
                                "Hide"
                            } else {
                                "Change"
                            })
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.notify.notif_sound_picker_open =
                                    !this.notify.notif_sound_picker_open;
                                if this.notify.notif_sound_picker_open
                                    && let Some(live) = this.live.as_mut()
                                {
                                    let _ = live.driver.maybe_fetch_notification_sounds();
                                }
                                cx.notify();
                            })),
                    ),
            );
        if self.notify.notif_sound_picker_open
            && let Some(chat_id) = open_chat
        {
            let current = if chat_settings.use_default_sound {
                SoundChoice::Default
            } else if chat_settings.sound_id == 0 {
                SoundChoice::Disabled
            } else {
                SoundChoice::Custom(chat_settings.sound_id)
            };
            panel = panel.child(self.notification_sound_picker(
                cx,
                SoundPickerTarget::Chat(chat_id),
                current,
                &saved_sounds,
            ));
        }
        // Parity slice (`parity:stories-notify-settings`): per-chat story
        // controls — mute toggle, poster toggle, and a story-sound picker
        // reusing the saved-sound picker with a story target.
        panel = panel.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(div().text_sm().child("Mute story notifications"))
                .child(
                    Switch::new("notif-story-mute-toggle")
                        .checked(story_muted)
                        .accessibility_label("Mute story notifications")
                        .on_click(cx.listener(move |this, &on, _, cx| {
                            if let Some(chat_id) = open_chat {
                                this.apply_chat_story_mute(chat_id, on, cx);
                            }
                        })),
                ),
        );
        panel = panel.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(div().text_sm().child("Show story poster"))
                .child(
                    Switch::new("notif-story-poster-toggle")
                        .checked(story_poster_on)
                        .accessibility_label("Show story poster")
                        .on_click(cx.listener(move |this, &on, _, cx| {
                            if let Some(chat_id) = open_chat {
                                this.apply_chat_story_poster(chat_id, on, cx);
                            }
                        })),
                ),
        );
        panel = panel.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .child(format!("Story sound: {story_sound_label}")),
                )
                .child(
                    Button::new("notif-story-sound-picker-toggle")
                        .small()
                        .label(if self.notify.story_sound_picker_open {
                            "Hide"
                        } else {
                            "Change"
                        })
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.notify.story_sound_picker_open =
                                !this.notify.story_sound_picker_open;
                            if this.notify.story_sound_picker_open
                                && let Some(live) = this.live.as_mut()
                            {
                                let _ = live.driver.maybe_fetch_notification_sounds();
                            }
                            cx.notify();
                        })),
                ),
        );
        if self.notify.story_sound_picker_open
            && let Some(chat_id) = open_chat
        {
            let current = if chat_settings.use_default_story_sound {
                SoundChoice::Default
            } else if chat_settings.story_sound_id == 0 {
                SoundChoice::Disabled
            } else {
                SoundChoice::Custom(chat_settings.story_sound_id)
            };
            panel = panel.child(self.notification_sound_picker(
                cx,
                SoundPickerTarget::ChatStory(chat_id),
                current,
                &saved_sounds,
            ));
        }
        panel.child(
            Button::new("notif-open-defaults")
                .small()
                .label("Defaults for all chats\u{2026}")
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.notify.notification_defaults_open = true;
                    if let Some(live) = this.live.as_mut() {
                        let _ = live.driver.maybe_fetch_scope_notification_settings();
                        let _ = live.driver.maybe_fetch_notification_sounds();
                        for scope in NotificationSettingsScope::ALL {
                            let _ = live.driver.maybe_fetch_notification_exceptions(scope);
                        }
                    }
                    cx.notify();
                })),
        )
    }
}
