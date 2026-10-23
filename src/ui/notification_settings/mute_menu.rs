//! The per-chat mute menu.

use super::*;
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr, tr_with, trn};

impl QuillApp {
    pub(in crate::ui) fn close_mute_menu(&mut self, cx: &mut Context<Self>) {
        self.notify.mute_menu_open = false;
        self.notify.mute_chat = None;
        self.notify.mute_custom_open = false;
        self.notify.notif_sound_picker_open = false;
        cx.notify();
    }

    /// The Mute panel for a chat row's menu: keyed to that chat, whatever
    /// chat is open (tdesktop opens a box for the row's peer).
    pub(in crate::ui) fn open_mute_menu_for(
        &mut self,
        chat_id: ChatId,
        custom: bool,
        cx: &mut Context<Self>,
    ) {
        self.open_mute_menu(cx);
        self.notify.mute_chat = Some(chat_id);
        self.ensure_chat_volume_slider(chat_id, cx);
        self.notify.mute_custom_open = custom;
        self.notify.notif_sound_picker_open = !custom;
        self.queue_mute_time_inputs(cx);
        cx.notify();
    }

    pub(in crate::ui) fn open_mute_menu(&mut self, cx: &mut Context<Self>) {
        self.notify.mute_menu_open = true;
        self.notify.mute_chat = None;
        self.notify.mute_custom_open = false;
        self.queue_mute_time_inputs(cx);
        if let Some(chat_id) = self.session().and_then(|session| session.open_chat) {
            self.ensure_chat_volume_slider(chat_id, cx);
        }
        self.connection
            .set_note(StatusNote::silent(tr(keys::Q_PREFS_MUTE_FOR)));
        cx.notify();
    }

    pub(in crate::ui) fn apply_chat_mute(
        &mut self,
        chat_id: ChatId,
        mute_for: i32,
        cx: &mut Context<Self>,
    ) {
        self.notify.mute_menu_open = false;
        self.notify.mute_chat = None;
        if let Some(live) = self.live.as_mut() {
            let result = live.driver.set_chat_mute_for(chat_id, mute_for);
            self.connection.set_note(match result {
                Ok(_) if mute_for == 0 => StatusNote::silent(tr(keys::Q_PREFS_UNMUTING)),
                Ok(_) => StatusNote::silent(tr(keys::Q_PREFS_MUTING)),
                Err(_) => StatusNote::error(tr(keys::Q_PREFS_COULD_NOT_CHANGE_MUTE)),
            });
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
            self.connection.set_note(if mute_for == 0 {
                StatusNote::silent("unmuted")
            } else {
                StatusNote::silent("muted")
            });
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
        let open_chat = self
            .notify
            .mute_chat
            .or_else(|| session.as_ref().and_then(|s| s.open_chat));
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
            .map(|s| s.settings.saved_notification_sounds.clone())
            .unwrap_or_default();
        let status = tr_with(
            keys::Q_PREFS_SOUND_PREVIEWS,
            &[
                (
                    "is_muted_forever",
                    (if muted {
                        if chat_settings.is_muted_forever() {
                            tr(keys::Q_PREFS_MUTED_FOREVER)
                        } else {
                            tr(keys::NOTIFICATION_EXCEPTIONS_MUTED)
                        }
                    } else {
                        tr(keys::NOTIFICATION_EXCEPTIONS_UNMUTED)
                    }),
                ),
                ("sound_label", &sound_label.to_string()),
                (
                    "if_preview_on_on_else_off",
                    (if preview_on { "on" } else { "off" }),
                ),
            ],
        );

        let presets = [
            (trn(keys::HOURS, 1), MUTE_FOR_1_HOUR),
            (trn(keys::HOURS, 8), MUTE_FOR_8_HOURS),
            (trn(keys::DAYS, 2), MUTE_FOR_2_DAYS),
            (tr(keys::Q_PREFS_FOREVER).to_string(), MUTE_FOREVER),
        ];
        let sound_disabled = !chat_settings.use_default_sound && chat_settings.sound_id == 0;
        let custom_open = self.notify.mute_custom_open;
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
                .label(tr(keys::Q_PREFS_CUSTOM))
                .outline()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.notify.mute_custom_open = !this.notify.mute_custom_open;
                    if this.notify.mute_custom_open {
                        this.reset_mute_time_inputs(window, cx);
                    }
                    cx.notify();
                })),
        );
        if muted {
            preset_row = preset_row.child(
                Button::new("mute-unmute")
                    .small()
                    .label(tr(keys::MUTE_MENU_UNMUTE))
                    .outline()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(chat_id) = open_chat {
                            this.apply_chat_mute(chat_id, 0, cx);
                        }
                    })),
            );
        }
        // tdesktop `MuteBox`: the fields, then "Mute" (or "Unmute" at zero).
        let custom_row = custom_open
            .then(|| self.mute_time_value(cx).zip(self.mute_time_fields(cx)))
            .flatten()
            .map(|(custom, fields)| {
                let mute_for = custom.mute_for();
                div()
                    .id("mute-custom")
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(keys::MUTE_BOX_TITLE)),
                    )
                    .child(fields)
                    .child(
                        div().flex().child(
                            Button::new("mute-custom-apply")
                                .small()
                                .primary()
                                .label(if mute_for == 0 {
                                    tr(keys::MUTE_MENU_UNMUTE)
                                } else {
                                    tr(keys::MUTE_MENU_MUTE)
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(chat_id) = open_chat {
                                        let mute_for = this
                                            .mute_time_value(cx)
                                            .map_or(0, |custom| custom.mute_for());
                                        this.notify.mute_custom_open = false;
                                        this.apply_chat_mute(chat_id, mute_for, cx);
                                    }
                                })),
                        ),
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
                    .child(div().font_semibold().child(
                        match (self.notify.mute_chat, open_chat_summary) {
                            (Some(_), Some(chat)) => {
                                format!("{} · {}", tr(keys::CHART_TITLE_MUTE), chat.title)
                            }
                            _ => tr(keys::CHART_TITLE_MUTE).to_string(),
                        },
                    ))
                    .child(
                        Button::new("close-mute-menu")
                            .icon(gpui_kit::assets::IconName::X)
                            .tooltip(tr(keys::CLOSE))
                            .accessibility_label(tr(keys::CLOSE))
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
                    .child(tr(keys::Q_PREFS_MUTE_FOR_2)),
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
                            .child(tr(keys::Q_PREFS_SHOW_MESSAGE_PREVIEW_IN_NOTIFICATIONS)),
                    )
                    .child(
                        // Phase 6: kit Switch (was: On/Off ghost button).
                        Switch::new("notif-preview-toggle")
                            .checked(preview_on)
                            .accessibility_label(tr(
                                keys::Q_PREFS_SHOW_MESSAGE_PREVIEW_IN_NOTIFICATIONS,
                            ))
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
                    .child(div().text_sm().child(tr_with(
                        keys::Q_PREFS_NOTIFICATION_SOUND,
                        &[("sound_label", &sound_label.to_string())],
                    )))
                    .child(
                        Button::new("notif-sound-picker-toggle")
                            .small()
                            .label(if self.notify.notif_sound_picker_open {
                                tr(keys::Q_PREFS_HIDE)
                            } else {
                                tr(keys::Q_PREFS_CHANGE)
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
        if let Some(chat_id) = open_chat {
            panel = panel.children(self.chat_volume_row(chat_id, sound_disabled, cx));
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
                .child(
                    div()
                        .text_sm()
                        .child(tr(keys::Q_PREFS_MUTE_STORY_NOTIFICATIONS)),
                )
                .child(
                    Switch::new("notif-story-mute-toggle")
                        .checked(story_muted)
                        .accessibility_label(tr(keys::Q_PREFS_MUTE_STORY_NOTIFICATIONS))
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
                .child(div().text_sm().child(tr(keys::Q_PREFS_SHOW_STORY_POSTER)))
                .child(
                    Switch::new("notif-story-poster-toggle")
                        .checked(story_poster_on)
                        .accessibility_label(tr(keys::Q_PREFS_SHOW_STORY_POSTER))
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
                .child(div().text_sm().child(tr_with(
                    keys::Q_PREFS_STORY_SOUND,
                    &[("story_sound_label", &story_sound_label.to_string())],
                )))
                .child(
                    Button::new("notif-story-sound-picker-toggle")
                        .small()
                        .label(if self.notify.story_sound_picker_open {
                            tr(keys::Q_PREFS_HIDE)
                        } else {
                            tr(keys::Q_PREFS_CHANGE)
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
                .label(tr(keys::Q_PREFS_DEFAULTS_FOR_ALL_CHATS))
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.open_notification_settings(cx);
                })),
        )
    }
}
