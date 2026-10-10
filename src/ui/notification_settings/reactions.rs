//! Reaction notification settings.

use super::*;

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
            let Some(mut settings) = live.driver.session.reaction_notification_settings.clone()
            else {
                self.status_note = "reaction settings still loading…".into();
                cx.notify();
                return;
            };
            match kind {
                ReactionSourceKind::Message => settings.message_reaction_source = source,
                ReactionSourceKind::Story => settings.story_reaction_source = source,
                ReactionSourceKind::PollVote => settings.poll_vote_source = source,
            }
            let result = live.driver.send_reaction_notification_settings(&settings);
            self.status_note = match result {
                Ok(_) => "reaction notification setting updated…".into(),
                Err(_) => "could not change reaction notification setting".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            // Screenshot demo: apply locally so the dialog reflects it.
            let mut settings = session
                .reaction_notification_settings
                .clone()
                .unwrap_or_default();
            match kind {
                ReactionSourceKind::Message => settings.message_reaction_source = source,
                ReactionSourceKind::Story => settings.story_reaction_source = source,
                ReactionSourceKind::PollVote => settings.poll_vote_source = source,
            }
            session.reaction_notification_settings = Some(settings);
            self.status_note = "reaction notification setting updated".into();
        }
        cx.notify();
    }

    /// Parity slice: apply the reaction-notification sound
    /// (`setReactionNotificationSettings`).
    pub(in crate::ui) fn apply_reaction_sound(&mut self, sound_id: i64, cx: &mut Context<Self>) {
        self.defaults_sound_picker = None;
        if let Some(live) = self.live.as_mut() {
            // Guard: never send schema-defaults as current state — no
            // getter exists, so wait for the first
            // `updateReactionNotificationSettings` instead.
            let Some(mut settings) = live.driver.session.reaction_notification_settings.clone()
            else {
                self.status_note = "reaction settings still loading…".into();
                cx.notify();
                return;
            };
            settings.sound_id = sound_id;
            let result = live.driver.send_reaction_notification_settings(&settings);
            self.status_note = match result {
                Ok(_) => "reaction sound updated…".into(),
                Err(_) => "could not change reaction sound".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            // Screenshot demo: apply locally so the dialog reflects it.
            let mut settings = session
                .reaction_notification_settings
                .clone()
                .unwrap_or_default();
            settings.sound_id = sound_id;
            session.reaction_notification_settings = Some(settings);
            self.status_note = "reaction sound updated".into();
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
            let Some(mut settings) = live.driver.session.reaction_notification_settings.clone()
            else {
                self.status_note = "reaction settings still loading…".into();
                cx.notify();
                return;
            };
            settings.show_preview = show_preview;
            let result = live.driver.send_reaction_notification_settings(&settings);
            self.status_note = match result {
                Ok(_) => "reaction preview updated…".into(),
                Err(_) => "could not change reaction preview".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            // Screenshot demo: apply locally so the dialog reflects it.
            let mut settings = session
                .reaction_notification_settings
                .clone()
                .unwrap_or_default();
            settings.show_preview = show_preview;
            session.reaction_notification_settings = Some(settings);
            self.status_note = "reaction preview updated".into();
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
            .and_then(|s| s.reaction_notification_settings.clone())
            .unwrap_or_default();
        let loaded = self
            .session()
            .is_some_and(|s| s.reaction_notification_settings.is_some());
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
                    .child(div().font_semibold().text_sm().child("Reactions"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(if loaded {
                                "Set".to_string()
                            } else {
                                "Loading…".to_string()
                            }),
                    ),
            );
        for (kind, label) in [
            (ReactionSourceKind::Message, "Message reactions"),
            (ReactionSourceKind::Story, "Story reactions"),
            (ReactionSourceKind::PollVote, "Poll votes"),
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
                            .child("Show sender and emoji in notification"),
                    )
                    .child(
                        // Phase 6: kit Switch.
                        Switch::new("reaction-preview")
                            .checked(settings.show_preview)
                            .accessibility_label("Show sender and emoji in reaction notifications")
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
                            .child(format!("Sound: {sound_label}")),
                    )
                    .child(
                        Button::new("reaction-sound-toggle")
                            .small()
                            .label(
                                if self.defaults_sound_picker == Some(SoundPickerTarget::Reaction) {
                                    "Hide"
                                } else {
                                    "Change"
                                },
                            )
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.defaults_sound_picker = if this.defaults_sound_picker
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
        if self.defaults_sound_picker == Some(SoundPickerTarget::Reaction) {
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
            ("None", ReactionNotificationSource::None),
            ("Contacts", ReactionNotificationSource::Contacts),
            ("Everyone", ReactionNotificationSource::All),
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
