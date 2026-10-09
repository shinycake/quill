//! Notification settings and queueing.
use super::*;

impl Session {
    /// Parity slice: scope-defaulted settings for a chat's scope — the fetched
    /// `ScopeNotificationSettings`, or the schema defaults while the
    /// `getScopeNotificationSettings` fetch is still in flight. Chats keep
    /// `use_default_*` flags until the user overrides one (Unigram clones
    /// settings and clears the default flag), so the scope values are what
    /// `chatNotificationSettings` means when a flag is set (td_api.tl line
    /// 3348: "If true, the value for the relevant type of chat ... is used
    /// instead of mute_for").
    pub(crate) fn scope_settings_for(
        &self,
        scope: NotificationSettingsScope,
    ) -> ScopeNotificationSettings {
        self.scope_notification_settings
            .get(&scope)
            .cloned()
            .unwrap_or_default()
    }

    /// Effective mute for the toast/sound decisions: the chat's own
    /// exception mute, or the scope default's `mute_for` when the chat keeps
    /// `use_default_mute_for` (td_api.tl line 3348).
    // Public (not just crate-visible): the `ui` binary crate calls these.
    pub fn effective_muted(&self, chat: &ChatSummary) -> bool {
        if chat.is_muted() {
            return true;
        }
        let settings = &chat.notification_settings;
        if !settings.use_default_mute_for {
            return false;
        }
        let scope = scope_for_chat_kind(&chat.kind);
        self.scope_settings_for(scope).mute_for > 0
    }

    /// Effective message-preview allowance: the chat's own flag, or the
    /// scope default's `show_preview` when the chat keeps
    /// `use_default_show_preview` (td_api.tl line 3350).
    pub fn effective_preview_allowed(&self, chat: &ChatSummary) -> bool {
        let settings = &chat.notification_settings;
        if !settings.use_default_show_preview {
            return settings.show_preview;
        }
        let scope = scope_for_chat_kind(&chat.kind);
        self.scope_settings_for(scope).show_preview
    }

    /// Parity slice (`parity:stories-notify-settings`): effective
    /// story-notification mute — the chat's own flag, or the scope
    /// default's `mute_stories` when the chat keeps
    /// `use_default_mute_stories`.
    pub fn effective_story_muted(&self, chat: &ChatSummary) -> bool {
        let settings = &chat.notification_settings;
        if !settings.use_default_mute_stories {
            return settings.mute_stories;
        }
        let scope = scope_for_chat_kind(&chat.kind);
        self.scope_settings_for(scope).mute_stories
    }

    /// Parity slice (`parity:stories-notify-settings`): effective
    /// story-poster allowance — the chat's own flag, or the scope
    /// default's `show_story_poster` when the chat keeps
    /// `use_default_show_story_poster`.
    pub fn effective_story_poster(&self, chat: &ChatSummary) -> bool {
        let settings = &chat.notification_settings;
        if !settings.use_default_show_story_poster {
            return settings.show_story_poster;
        }
        let scope = scope_for_chat_kind(&chat.kind);
        self.scope_settings_for(scope).show_story_poster
    }

    /// Phase 8.1: pure notify / don't-notify decision for an `updateNewMessage`.
    /// Both the UI's `app_active` write and the reducer run on the UI thread,
    /// so no locking is needed. Returns `None` when the chat is unknown (no
    /// title, no verified mute/read state) rather than guessing.
    pub(crate) fn notification_for_new_message(
        &self,
        message: &ParsedMessage,
    ) -> Option<OsNotification> {
        if !self.desktop_notifications {
            return None;
        }
        let chat = self.chats.get(&message.chat_id.0)?;
        let chat_muted = self.effective_muted(chat);
        let chat_preview_allowed = self.effective_preview_allowed(chat);
        notify::decide_notify(&notify::NotifyInput {
            message,
            chat_title: Some(&chat.title),
            chat_muted,
            last_read_inbox_message_id: Some(chat.last_read_inbox_message_id),
            open_chat: self.open_chat,
            app_active: self.app_active,
            hide_previews: self.hide_notification_previews,
            chat_preview_allowed,
        })
    }

    /// Parity slice: pure play / don't-play decision for a notification's
    /// sound. Made at message-arrival time, together with the toast decision,
    /// so the sound reflects the mute/focus state the toast was decided on.
    pub(crate) fn notification_sound_for(
        &self,
        chat: &ChatSummary,
    ) -> Option<notify::NotificationSoundKind> {
        // Parity slice: in-app sounds toggle (tdesktop "Play sounds").
        // Client-side preference — when off, no sound is decided for
        // any notification.
        if !self.inapp_sounds_enabled {
            return None;
        }
        let settings = &chat.notification_settings;
        let scope = scope_for_chat_kind(&chat.kind);
        let scope_sound_id = self
            .scope_notification_settings
            .get(&scope)
            .map(|s| s.sound_id);
        notify::decide_notification_sound(&notify::SoundInput {
            app_active: self.app_active,
            chat_muted: self.effective_muted(chat),
            use_default_sound: settings.use_default_sound,
            chat_sound_id: settings.sound_id,
            scope_sound_id,
        })
    }

    /// Phase 8.1: append with same-chat burst coalescing; the first
    /// message's sound wins so a burst plays exactly once.
    pub(crate) fn queue_notification_with_sound(
        &mut self,
        notification: OsNotification,
        sound: Option<notify::NotificationSoundKind>,
    ) {
        self.shown_notification_chats.insert(notification.chat_id);
        notify::coalesce_notification_with_sound(
            &mut self.pending_notifications,
            notification,
            sound,
        );
    }

    /// The chat was read (here or elsewhere) or TDLib removed its
    /// notifications: drop what is still queued and ask the UI to
    /// withdraw anything already shown.
    pub(crate) fn clear_chat_notifications(&mut self, chat_id: ChatId) {
        self.pending_notifications
            .retain(|queued| queued.chat_id != chat_id);
        if self.shown_notification_chats.remove(&chat_id)
            && !self.pending_notification_clears.contains(&chat_id)
        {
            self.pending_notification_clears.push(chat_id);
        }
    }

    /// A new unread reaction on one of our messages: notify when the
    /// reaction settings allow it (`updateReactionNotificationSettings`;
    /// nothing is shown until they have arrived, as their default source
    /// is "none").
    pub(crate) fn queue_reaction_notification(
        &mut self,
        chat_id: ChatId,
        reaction: &crate::telegram::envelope::UnreadReaction,
    ) {
        if !self.desktop_notifications {
            return;
        }
        let (Some(settings), Some(chat)) = (
            self.reaction_notification_settings.as_ref(),
            self.chats.get(&chat_id.0),
        ) else {
            return;
        };
        let (sender_name, sender_is_contact) = match reaction.sender {
            Some(MessageSender::User { user_id }) => match self.user(user_id) {
                Some(user) => (Some(user.display_name()), user.is_contact),
                None => (None, false),
            },
            Some(MessageSender::Chat { .. }) | None => (None, false),
        };
        let notification = notify::decide_reaction_notify(&notify::ReactionNotifyInput {
            chat_id,
            chat_title: &chat.title,
            source: settings.message_reaction_source,
            sender_name: sender_name.as_deref(),
            sender_is_contact,
            emoji: reaction.reaction_type.emoji_text(),
            show_preview: settings.show_preview && !self.hide_notification_previews,
            chat_muted: self.effective_muted(chat),
            app_active: self.app_active,
            open_chat: self.open_chat,
        });
        if let Some(notification) = notification {
            // The reaction sound follows `reactionNotificationSettings`:
            // 0 = silent, anything else the default tone.
            let sound = (self.inapp_sounds_enabled && settings.sound_id != 0 && !self.app_active)
                .then_some(notify::NotificationSoundKind::Default);
            self.queue_notification_with_sound(notification, sound);
        }
    }
}
