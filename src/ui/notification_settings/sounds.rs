//! Per-chat sound, preview and story settings, and the sound picker.

use super::*;

impl QuillApp {
    /// Parity slice: current sound choice for a chat, for the notifications
    /// panel summary and picker checkmarks.
    pub(in crate::ui) fn notification_sound_label(
        &self,
        settings: &ChatNotificationSettings,
    ) -> String {
        if settings.use_default_sound {
            return "Default".to_string();
        }
        if settings.sound_id == 0 {
            return "None".to_string();
        }
        self.session()
            .and_then(|s| {
                s.settings
                    .saved_notification_sounds
                    .iter()
                    .find(|sound| sound.id == settings.sound_id)
                    .map(|sound| sound.title.clone())
            })
            .unwrap_or_else(|| "Custom".to_string())
    }

    /// Parity slice (`parity:stories-notify-settings`): current story-sound
    /// choice for a chat, for the notifications panel summary and picker
    /// checkmarks.
    pub(in crate::ui) fn story_sound_label(&self, settings: &ChatNotificationSettings) -> String {
        if settings.use_default_story_sound {
            return "Default".to_string();
        }
        if settings.story_sound_id == 0 {
            return "None".to_string();
        }
        self.session()
            .and_then(|s| {
                s.settings
                    .saved_notification_sounds
                    .iter()
                    .find(|sound| sound.id == settings.story_sound_id)
                    .map(|sound| sound.title.clone())
            })
            .unwrap_or_else(|| "Custom".to_string())
    }

    /// Parity slice: apply a sound choice to the open chat
    /// (`setChatNotificationSettings`).
    pub(in crate::ui) fn apply_chat_sound(
        &mut self,
        chat_id: ChatId,
        use_default_sound: bool,
        sound_id: i64,
        cx: &mut Context<Self>,
    ) {
        self.notify.notif_sound_picker_open = false;
        if self.live.is_some() {
            let result = self.live.as_mut().expect("live").driver.set_chat_sound(
                chat_id,
                use_default_sound,
                sound_id,
            );
            self.connection.status_note = match result {
                Ok(_) => "sound updated…".into(),
                Err(_) => "could not change sound".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_notification_settings(
                chat_id,
                |settings| {
                    settings.use_default_sound = use_default_sound;
                    settings.sound_id = sound_id;
                },
                cx,
            );
            self.connection.status_note = "sound updated".into();
            cx.notify();
        }
    }

    /// Parity slice: apply a message-preview exception to the open chat
    /// (`setChatNotificationSettings`).
    pub(in crate::ui) fn apply_chat_preview(
        &mut self,
        chat_id: ChatId,
        show_preview: bool,
        cx: &mut Context<Self>,
    ) {
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .set_chat_show_preview(chat_id, show_preview);
            self.connection.status_note = match result {
                Ok(_) => "preview setting updated…".into(),
                Err(_) => "could not change preview".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_notification_settings(
                chat_id,
                |settings| {
                    settings.use_default_show_preview = false;
                    settings.show_preview = show_preview;
                },
                cx,
            );
            self.connection.status_note = "preview setting updated".into();
            cx.notify();
        }
    }

    /// Parity slice (`parity:stories-notify-settings`): apply a
    /// story-mute exception to the open chat (`setChatNotificationSettings`).
    pub(in crate::ui) fn apply_chat_story_mute(
        &mut self,
        chat_id: ChatId,
        mute_stories: bool,
        cx: &mut Context<Self>,
    ) {
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .set_chat_story_mute(chat_id, mute_stories);
            self.connection.status_note = match result {
                Ok(_) => "story mute updated…".into(),
                Err(_) => "could not change story mute".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_notification_settings(
                chat_id,
                |settings| {
                    settings.use_default_mute_stories = false;
                    settings.mute_stories = mute_stories;
                },
                cx,
            );
            self.connection.status_note = "story mute updated".into();
            cx.notify();
        }
    }

    /// Parity slice (`parity:stories-notify-settings`): apply a
    /// story-poster exception to the open chat (`setChatNotificationSettings`).
    pub(in crate::ui) fn apply_chat_story_poster(
        &mut self,
        chat_id: ChatId,
        show_story_poster: bool,
        cx: &mut Context<Self>,
    ) {
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .set_chat_story_poster(chat_id, show_story_poster);
            self.connection.status_note = match result {
                Ok(_) => "story poster setting updated…".into(),
                Err(_) => "could not change story poster".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_notification_settings(
                chat_id,
                |settings| {
                    settings.use_default_show_story_poster = false;
                    settings.show_story_poster = show_story_poster;
                },
                cx,
            );
            self.connection.status_note = "story poster setting updated".into();
            cx.notify();
        }
    }

    /// Parity slice (`parity:stories-notify-settings`): apply a story-sound
    /// choice to the open chat (`setChatNotificationSettings`).
    pub(in crate::ui) fn apply_chat_story_sound(
        &mut self,
        chat_id: ChatId,
        use_default_story_sound: bool,
        story_sound_id: i64,
        cx: &mut Context<Self>,
    ) {
        self.notify.story_sound_picker_open = false;
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .set_chat_story_sound(chat_id, use_default_story_sound, story_sound_id);
            self.connection.status_note = match result {
                Ok(_) => "story sound updated…".into(),
                Err(_) => "could not change story sound".into(),
            };
            cx.notify();
            return;
        }
        if self.demo_session.is_some() {
            self.apply_demo_notification_settings(
                chat_id,
                |settings| {
                    settings.use_default_story_sound = use_default_story_sound;
                    settings.story_sound_id = story_sound_id;
                },
                cx,
            );
            self.connection.status_note = "story sound updated".into();
            cx.notify();
        }
    }

    /// Parity slice: preview a saved notification sound immediately — play
    /// the MP3 when local, otherwise download it and play on completion
    /// (via `Session::pending_sound_plays`).
    pub(in crate::ui) fn preview_saved_sound(&mut self, sound_id: i64) {
        let sound = match self.live.as_mut() {
            Some(live) => crate::ui::audio::notification_sound(
                live.driver
                    .resolve_notification_sound(NotificationSoundKind::Custom(sound_id)),
            ),
            // Screenshot demo: no TDLib files exist — the tone stands in.
            None => Some(crate::ui::audio::NotificationSound::DefaultTone),
        };
        if let Some(sound) = sound {
            self.notify.notification_sounds.play(sound);
        }
    }

    /// Parity slice: the saved-sound picker shared by the per-chat panel and
    /// the scope defaults dialog. `getSavedNotificationSounds` says: "If a
    /// sound isn't in the list, then default sound needs to be used" — so
    /// Default / None are always offered first.
    pub(in crate::ui) fn notification_sound_picker(
        &self,
        cx: &mut Context<Self>,
        target: SoundPickerTarget,
        current: SoundChoice,
        saved_sounds: &[NotificationSound],
    ) -> AnyElement {
        let list_id = match target {
            SoundPickerTarget::Chat(_) => "notif-sound-list".to_string(),
            SoundPickerTarget::ChatStory(_) => "notif-story-sound-list".to_string(),
            SoundPickerTarget::Scope(scope) => format!("scope-sound-list-{scope:?}"),
            SoundPickerTarget::Reaction => "reaction-sound-list".to_string(),
        };
        let mut list = div().id(list_id).flex().flex_col().gap_1().py_1();
        list = list.child(self.sound_picker_row(
            cx,
            target,
            SoundChoice::Default,
            current,
            "Default",
            "Quill default tone",
            None,
        ));
        list = list.child(self.sound_picker_row(
            cx,
            target,
            SoundChoice::Disabled,
            current,
            "None",
            "No sound",
            None,
        ));
        for sound in saved_sounds {
            let subtitle = format!("{} ({}s)", sound.title, sound.duration.max(0));
            list = list.child(self.sound_picker_row(
                cx,
                target,
                SoundChoice::Custom(sound.id),
                current,
                &sound.title,
                &subtitle,
                Some(sound.id),
            ));
        }
        list.into_any_element()
    }

    /// Parity slice: one row of the sound picker. The `▶` preview button
    /// plays the sound without selecting it.
    pub(in crate::ui) fn sound_picker_row(
        &self,
        cx: &mut Context<Self>,
        target: SoundPickerTarget,
        choice: SoundChoice,
        current: SoundChoice,
        title: &str,
        subtitle: &str,
        preview_sound_id: Option<i64>,
    ) -> AnyElement {
        let selected = choice == current;
        let row_id = match (target, choice) {
            (SoundPickerTarget::Chat(_), SoundChoice::Default) => "sound-pick-chat-default",
            (SoundPickerTarget::Chat(_), SoundChoice::Disabled) => "sound-pick-chat-none",
            (SoundPickerTarget::Chat(_), SoundChoice::Custom(_)) => "sound-pick-chat-custom",
            (SoundPickerTarget::ChatStory(_), SoundChoice::Default) => {
                "sound-pick-chat-story-default"
            }
            (SoundPickerTarget::ChatStory(_), SoundChoice::Disabled) => {
                "sound-pick-chat-story-none"
            }
            (SoundPickerTarget::ChatStory(_), SoundChoice::Custom(_)) => {
                "sound-pick-chat-story-custom"
            }
            (SoundPickerTarget::Scope(_), SoundChoice::Default) => "sound-pick-scope-default",
            (SoundPickerTarget::Scope(_), SoundChoice::Disabled) => "sound-pick-scope-none",
            (SoundPickerTarget::Scope(_), SoundChoice::Custom(_)) => "sound-pick-scope-custom",
            (SoundPickerTarget::Reaction, SoundChoice::Default) => "sound-pick-reaction-default",
            (SoundPickerTarget::Reaction, SoundChoice::Disabled) => "sound-pick-reaction-none",
            (SoundPickerTarget::Reaction, SoundChoice::Custom(_)) => "sound-pick-reaction-custom",
        };
        let mut row = div()
            .id(format!("{row_id}-row"))
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_2()
            .py_1()
            .rounded_md()
            .role(gpui_kit::Role::Button)
            .aria_label(format!(
                "{title}{}",
                if selected { ", selected" } else { "" }
            ))
            .tab_index(0)
            .cursor_pointer()
            .hover(|style| style.bg(cx.theme().accent.opacity(0.08)))
            .child(
                // Phase 6: kit Radio as the selection indicator (was: a "✓ "
                // prefix on the title). The whole row stays the click
                // target, exactly as before — the radio is display-only.
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(Radio::new(format!("{row_id}-radio")).checked(selected))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(div().text_sm().child(title.to_string()))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(subtitle.to_string()),
                            ),
                    ),
            );
        if let Some(sound_id) = preview_sound_id {
            row = row.child(
                Button::new((row_id, sound_id as u64))
                    .small()
                    .label("▶")
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, _| {
                        this.preview_saved_sound(sound_id);
                    })),
            );
        }
        row.on_click(cx.listener(move |this, _, _, cx| {
            this.apply_sound_choice(target, choice, cx);
        }))
        .into_any_element()
    }

    /// Parity slice: apply a picker choice to its target.
    pub(in crate::ui) fn apply_sound_choice(
        &mut self,
        target: SoundPickerTarget,
        choice: SoundChoice,
        cx: &mut Context<Self>,
    ) {
        match target {
            SoundPickerTarget::Chat(chat_id) => {
                let (use_default_sound, sound_id) = match choice {
                    SoundChoice::Default => (true, 0),
                    SoundChoice::Disabled => (false, 0),
                    SoundChoice::Custom(id) => (false, id),
                };
                self.apply_chat_sound(chat_id, use_default_sound, sound_id, cx);
            }
            // Parity slice (`parity:stories-notify-settings`): story sound
            // follows the same Default / None / custom shape as the message
            // sound (schema line 3350).
            SoundPickerTarget::ChatStory(chat_id) => {
                let (use_default_story_sound, story_sound_id) = match choice {
                    SoundChoice::Default => (true, 0),
                    SoundChoice::Disabled => (false, 0),
                    SoundChoice::Custom(id) => (false, id),
                };
                self.apply_chat_story_sound(chat_id, use_default_story_sound, story_sound_id, cx);
            }
            SoundPickerTarget::Scope(scope) => {
                let sound_id = match choice {
                    // Scope `-1` = app-dependent default (schema line 3368).
                    SoundChoice::Default => -1,
                    SoundChoice::Disabled => 0,
                    SoundChoice::Custom(id) => id,
                };
                self.apply_scope_sound(scope, sound_id, cx);
            }
            SoundPickerTarget::Reaction => {
                let sound_id = match choice {
                    // Reaction `-1` = app-dependent default (schema line 3394).
                    SoundChoice::Default => -1,
                    SoundChoice::Disabled => 0,
                    SoundChoice::Custom(id) => id,
                };
                self.apply_reaction_sound(sound_id, cx);
            }
        }
    }
}
