//! Reaction notification settings.

use super::*;
use quill::lang::{keys, tr, tr_with};

impl QuillApp {
    /// Parity slice: apply a reaction-notification source
    /// (`setReactionNotificationSettings`).
    pub(in crate::ui) fn apply_reaction_source(
        &mut self,
        kind: ReactionSourceKind,
        source: ReactionNotificationSource,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            // Guard: never send schema-defaults as current state — no
            // getter exists, so wait for the first
            // `updateReactionNotificationSettings` instead.
            let Some(mut settings) = live
                .driver
                .session
                .settings
                .reaction_notification_settings
                .clone()
            else {
                self.connection.status_note =
                    tr(keys::Q_PREFS_REACTION_SETTINGS_STILL_LOADING).into();
                cx.notify();
                return;
            };
            match kind {
                ReactionSourceKind::Message => settings.message_reaction_source = source,
                ReactionSourceKind::Story => settings.story_reaction_source = source,
                ReactionSourceKind::PollVote => settings.poll_vote_source = source,
            }
            let result = live.driver.send_reaction_notification_settings(&settings);
            self.connection.status_note = match result {
                Ok(_) => tr(keys::Q_PREFS_REACTION_NOTIFICATION_SETTING_UPDATED).into(),
                Err(_) => tr(keys::Q_PREFS_COULD_NOT_CHANGE_REACTION_NOTIFICATION).into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            // Screenshot demo: apply locally so the dialog reflects it.
            let mut settings = session
                .settings
                .reaction_notification_settings
                .clone()
                .unwrap_or_default();
            match kind {
                ReactionSourceKind::Message => settings.message_reaction_source = source,
                ReactionSourceKind::Story => settings.story_reaction_source = source,
                ReactionSourceKind::PollVote => settings.poll_vote_source = source,
            }
            session.settings.reaction_notification_settings = Some(settings);
            self.connection.status_note =
                tr(keys::Q_PREFS_REACTION_NOTIFICATION_SETTING_UPDATED_2).into();
        }
        cx.notify();
    }

    /// Parity slice: apply the reaction-notification sound
    /// (`setReactionNotificationSettings`).
    pub(in crate::ui) fn apply_reaction_sound(&mut self, sound_id: i64, cx: &mut Context<Self>) {
        self.notify.defaults_sound_picker = None;
        if let Some(live) = self.live.as_mut() {
            // Guard: never send schema-defaults as current state — no
            // getter exists, so wait for the first
            // `updateReactionNotificationSettings` instead.
            let Some(mut settings) = live
                .driver
                .session
                .settings
                .reaction_notification_settings
                .clone()
            else {
                self.connection.status_note =
                    tr(keys::Q_PREFS_REACTION_SETTINGS_STILL_LOADING).into();
                cx.notify();
                return;
            };
            settings.sound_id = sound_id;
            let result = live.driver.send_reaction_notification_settings(&settings);
            self.connection.status_note = match result {
                Ok(_) => tr(keys::Q_PREFS_REACTION_SOUND_UPDATED).into(),
                Err(_) => tr(keys::Q_PREFS_COULD_NOT_CHANGE_REACTION_SOUND).into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            // Screenshot demo: apply locally so the dialog reflects it.
            let mut settings = session
                .settings
                .reaction_notification_settings
                .clone()
                .unwrap_or_default();
            settings.sound_id = sound_id;
            session.settings.reaction_notification_settings = Some(settings);
            self.connection.status_note = tr(keys::Q_PREFS_REACTION_SOUND_UPDATED_2).into();
        }
        cx.notify();
    }

    /// Parity slice: apply the reaction-notification preview flag
    /// (`setReactionNotificationSettings`).
    pub(in crate::ui) fn apply_reaction_preview(
        &mut self,
        show_preview: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            // Guard: never send schema-defaults as current state — no
            // getter exists, so wait for the first
            // `updateReactionNotificationSettings` instead.
            let Some(mut settings) = live
                .driver
                .session
                .settings
                .reaction_notification_settings
                .clone()
            else {
                self.connection.status_note =
                    tr(keys::Q_PREFS_REACTION_SETTINGS_STILL_LOADING).into();
                cx.notify();
                return;
            };
            settings.show_preview = show_preview;
            let result = live.driver.send_reaction_notification_settings(&settings);
            self.connection.status_note = match result {
                Ok(_) => tr(keys::Q_PREFS_REACTION_PREVIEW_UPDATED).into(),
                Err(_) => tr(keys::Q_PREFS_COULD_NOT_CHANGE_REACTION_PREVIEW).into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            // Screenshot demo: apply locally so the dialog reflects it.
            let mut settings = session
                .settings
                .reaction_notification_settings
                .clone()
                .unwrap_or_default();
            settings.show_preview = show_preview;
            session.settings.reaction_notification_settings = Some(settings);
            self.connection.status_note = tr(keys::Q_PREFS_REACTION_PREVIEW_UPDATED_2).into();
        }
        cx.notify();
    }

    /// Parity slice: reaction + poll-vote notification settings section in
    /// the defaults dialog (`reactionNotificationSettings`, TDLib 1.8.67
    /// line 3396; no getter — the current values arrive as
    /// `updateReactionNotificationSettings`).
    pub(in crate::ui) fn reaction_settings_section(
        &self,
        cx: &mut Context<Self>,
        saved_sounds: &[NotificationSound],
    ) -> AnyElement {
        let settings: ReactionNotificationSettings = self
            .session()
            .and_then(|s| s.settings.reaction_notification_settings.clone())
            .unwrap_or_default();
        let loaded = self
            .session()
            .is_some_and(|s| s.settings.reaction_notification_settings.is_some());
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

        let mut section = div()
            .id("reaction-settings-section")
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
                            .child(tr(keys::NOTIFICATION_REACTIONS)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(if loaded {
                                tr(keys::Q_PREFS_SET).to_string()
                            } else {
                                tr(keys::Q_PREFS_LOADING).to_string()
                            }),
                    ),
            );
        for (kind, label) in [
            (
                ReactionSourceKind::Message,
                tr(keys::Q_PREFS_MESSAGE_REACTIONS),
            ),
            (ReactionSourceKind::Story, tr(keys::Q_PREFS_STORY_REACTIONS)),
            (
                ReactionSourceKind::PollVote,
                tr(keys::NOTIFICATION_REACTIONS_POLL_VOTES),
            ),
        ] {
            section = section.child(self.reaction_source_row(cx, kind, label));
        }
        section = section
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
                            .child(tr(keys::Q_PREFS_SHOW_SENDER_AND_EMOJI_IN)),
                    )
                    .child(
                        // Phase 6: kit Switch.
                        Switch::new("reaction-preview")
                            .checked(settings.show_preview)
                            .accessibility_label(tr(keys::Q_PREFS_SHOW_SENDER_AND_EMOJI_IN_2))
                            .on_click(cx.listener(move |this, &on, _, cx| {
                                this.apply_reaction_preview(on, cx);
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
                        Button::new("reaction-sound-toggle")
                            .small()
                            .label(
                                if self.notify.defaults_sound_picker
                                    == Some(SoundPickerTarget::Reaction)
                                {
                                    tr(keys::Q_PREFS_HIDE)
                                } else {
                                    tr(keys::Q_PREFS_CHANGE)
                                },
                            )
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.notify.defaults_sound_picker =
                                    if this.notify.defaults_sound_picker
                                        == Some(SoundPickerTarget::Reaction)
                                    {
                                        None
                                    } else {
                                        Some(SoundPickerTarget::Reaction)
                                    };
                                cx.notify();
                            })),
                    ),
            );
        if self.notify.defaults_sound_picker == Some(SoundPickerTarget::Reaction) {
            section = section.child(self.notification_sound_picker(
                cx,
                SoundPickerTarget::Reaction,
                current_choice,
                saved_sounds,
            ));
        }
        section.into_any_element()
    }

    /// Parity slice: one reaction source's preset row (None / Contacts /
    /// Everyone), mirroring the scope mute presets.
    pub(in crate::ui) fn reaction_source_row(
        &self,
        cx: &mut Context<Self>,
        kind: ReactionSourceKind,
        label: &str,
    ) -> AnyElement {
        let mut row = div().flex().flex_wrap().gap_1().items_center();
        row = row.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(format!("{label}:")),
        );
        for (preset_label, source) in [
            (
                tr(keys::SETTINGS_NO_BLOCKED_USERS),
                ReactionNotificationSource::None,
            ),
            (
                tr(keys::MENU_CONTACTS),
                ReactionNotificationSource::Contacts,
            ),
            (tr(keys::Q_PREFS_EVERYONE), ReactionNotificationSource::All),
        ] {
            row = row.child(
                Button::new(format!("reaction-source-{kind:?}-{source:?}"))
                    .small()
                    .label(preset_label)
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.apply_reaction_source(kind, source, cx);
                    })),
            );
        }
        row.into_any_element()
    }
}
