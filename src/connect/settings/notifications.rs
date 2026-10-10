//! Connect driver: chat, scope and reaction notification settings and sounds.
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// notification settings and clears `use_default_mute_for` (Unigram).
    pub fn set_chat_mute_for(
        &mut self,
        chat_id: ChatId,
        mute_for: i32,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat) = self.session.chats.get(&chat_id.0) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if !chat.supported() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let settings = chat.notification_settings.clone().with_mute_for(mute_for);
        self.send_notification_settings(chat_id, &settings)
    }

    /// Unmute (`mute_for` 0, not "use default").
    pub fn unmute_chat(&mut self, chat_id: ChatId) -> Result<RequestId, ConnectSendError> {
        self.set_chat_mute_for(chat_id, 0)
    }

    /// Mute forever (`i32::MAX`, tdesktop `kMuteForeverValue`).
    pub fn mute_chat_forever(&mut self, chat_id: ChatId) -> Result<RequestId, ConnectSendError> {
        self.set_chat_mute_for(chat_id, MUTE_FOREVER)
    }

    fn send_notification_settings(
        &mut self,
        chat_id: ChatId,
        settings: &ChatNotificationSettings,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self
            .session
            .request(RequestPurpose::SetChatNotificationSettings, Some(chat_id));
        let json = set_chat_notification_settings(extra, chat_id, settings);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Parity slice: `getChatNotificationSettingsExceptions` (TDLib 1.8.67,
    /// line 13659) for a scope not yet loaded and not in flight — once per
    /// dialog open. `compare_sound=true` includes chats whose only
    /// non-default setting is the sound (the exceptions list view).
    pub fn maybe_fetch_notification_exceptions(
        &mut self,
        scope: NotificationSettingsScope,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.notification_exceptions.contains_key(&scope)
            || self
                .session
                .notification_exceptions_loading
                .contains(&scope)
            || self
                .session
                .requests
                .has_purpose_for_scope(RequestPurpose::GetChatNotificationSettingsExceptions, scope)
        {
            return Ok(());
        }
        let extra = self
            .session
            .request_for_scope(RequestPurpose::GetChatNotificationSettingsExceptions, scope);
        self.session.notification_exceptions_loading.insert(scope);
        if let Err(err) = self
            .sender
            .send_json(&get_chat_notification_settings_exceptions(
                extra, scope, true,
            ))
        {
            self.session.requests.take(extra);
            self.session.notification_exceptions_loading.remove(&scope);
            return Err(err);
        }
        Ok(())
    }

    /// Parity slice: reset a chat's notification settings to the scope
    /// defaults (`setChatNotificationSettings` with every `use_default_*`
    /// flag set). The change arrives back as
    /// `updateChatNotificationSettings`, which prunes the chat from the
    /// cached exceptions list.
    pub fn reset_chat_notification_settings(
        &mut self,
        chat_id: ChatId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.send_notification_settings(chat_id, &ChatNotificationSettings::default())
    }

    /// Parity slice: set the chat's notification-sound exception.
    /// `use_default_sound = true` keeps the scope default; `sound_id = 0`
    /// disables sound (schema line 3350).
    pub fn set_chat_sound(
        &mut self,
        chat_id: ChatId,
        use_default_sound: bool,
        sound_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat) = self.session.chats.get(&chat_id.0) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if !chat.supported() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let mut settings = chat.notification_settings.clone();
        settings.use_default_sound = use_default_sound;
        settings.sound_id = sound_id;
        self.send_notification_settings(chat_id, &settings)
    }

    /// Parity slice: set the chat's message-preview exception.
    pub fn set_chat_show_preview(
        &mut self,
        chat_id: ChatId,
        show_preview: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat) = self.session.chats.get(&chat_id.0) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if !chat.supported() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let mut settings = chat.notification_settings.clone();
        settings.use_default_show_preview = false;
        settings.show_preview = show_preview;
        self.send_notification_settings(chat_id, &settings)
    }

    /// Parity slice: set the chat's story-notification mute exception
    /// (`parity:stories-notify-settings`).
    pub fn set_chat_story_mute(
        &mut self,
        chat_id: ChatId,
        mute_stories: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat) = self.session.chats.get(&chat_id.0) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if !chat.supported() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let mut settings = chat.notification_settings.clone();
        settings.use_default_mute_stories = false;
        settings.mute_stories = mute_stories;
        self.send_notification_settings(chat_id, &settings)
    }

    /// Parity slice: set the chat's story-poster exception
    /// (`parity:stories-notify-settings`).
    pub fn set_chat_story_poster(
        &mut self,
        chat_id: ChatId,
        show_story_poster: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat) = self.session.chats.get(&chat_id.0) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if !chat.supported() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let mut settings = chat.notification_settings.clone();
        settings.use_default_show_story_poster = false;
        settings.show_story_poster = show_story_poster;
        self.send_notification_settings(chat_id, &settings)
    }

    /// Parity slice: set the chat's story-sound exception
    /// (`parity:stories-notify-settings`). `use_default_story_sound = true`
    /// keeps the scope default; `story_sound_id = 0` disables sound (schema
    /// line 3350).
    pub fn set_chat_story_sound(
        &mut self,
        chat_id: ChatId,
        use_default_story_sound: bool,
        story_sound_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat) = self.session.chats.get(&chat_id.0) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if !chat.supported() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let mut settings = chat.notification_settings.clone();
        settings.use_default_story_sound = use_default_story_sound;
        settings.story_sound_id = story_sound_id;
        self.send_notification_settings(chat_id, &settings)
    }

    /// Parity slice: `getSavedNotificationSounds` once per Ready (guarded by
    /// loaded / in-flight). Drives the sound picker and custom-sound
    /// playback.
    pub fn maybe_fetch_notification_sounds(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.saved_sounds_loaded
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetSavedNotificationSounds)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetSavedNotificationSounds, None);
        match self.sender.send_json(&get_saved_notification_sounds(extra)) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Parity slice: refetch the saved-sound list after
    /// `updateSavedNotificationSounds` marked it stale.
    pub fn refresh_notification_sounds_if_stale(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.session.saved_sounds_stale {
            return Ok(None);
        }
        self.session.saved_sounds_loaded = false;
        self.maybe_fetch_notification_sounds()
    }

    /// Parity slice: `getScopeNotificationSettings` for the scopes not yet
    /// loaded and not in flight — once per Ready.
    pub fn maybe_fetch_scope_notification_settings(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        for scope in NotificationSettingsScope::ALL {
            if self
                .session
                .scope_notification_settings
                .contains_key(&scope)
                || self.session.scope_settings_loading.contains(&scope)
                || self
                    .session
                    .requests
                    .has_purpose_for_scope(RequestPurpose::GetScopeNotificationSettings, scope)
            {
                continue;
            }
            let extra = self
                .session
                .request_for_scope(RequestPurpose::GetScopeNotificationSettings, scope);
            self.session.scope_settings_loading.insert(scope);
            if let Err(err) = self
                .sender
                .send_json(&get_scope_notification_settings(extra, scope))
            {
                self.session.requests.take(extra);
                self.session.scope_settings_loading.remove(&scope);
                return Err(err);
            }
        }
        Ok(())
    }

    /// Parity slice: `setScopeNotificationSettings` for one scope (full
    /// object; callers copy the current scope settings and change one
    /// field). The new values arrive as `updateScopeNotificationSettings`.
    pub fn send_scope_notification_settings(
        &mut self,
        scope: NotificationSettingsScope,
        settings: &ScopeNotificationSettings,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request_for_scope(RequestPurpose::SetScopeNotificationSettings, scope);
        match self
            .sender
            .send_json(&set_scope_notification_settings(extra, scope, settings))
        {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Parity slice: `setReactionNotificationSettings` (full object; callers
    /// copy the current settings and change one field). No getter exists —
    /// the new values arrive as `updateReactionNotificationSettings`.
    pub fn send_reaction_notification_settings(
        &mut self,
        settings: &ReactionNotificationSettings,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetReactionNotificationSettings, None);
        match self
            .sender
            .send_json(&set_reaction_notification_settings(extra, settings))
        {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Parity slice: `resetAllNotificationSettings` — resets all chat and
    /// scope notification settings to their default values. The new values
    /// arrive as `updateScopeNotificationSettings` /
    /// `updateChatNotificationSettings`; the ok arm drops the cached scope
    /// settings so the next fetch shows server-confirmed defaults.
    pub fn reset_all_notification_settings(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::ResetAllNotificationSettings, None);
        match self
            .sender
            .send_json(&reset_all_notification_settings(extra))
        {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Parity slice: resolve a notification sound to a playable form.
    /// A custom id missing from the saved list falls back to the default
    /// tone (per the `getSavedNotificationSounds` schema comment).
    pub fn resolve_notification_sound(&mut self, kind: NotificationSoundKind) -> SoundResolution {
        use SoundResolution as R;
        let sound_id = match kind {
            NotificationSoundKind::Default => return R::DefaultTone,
            NotificationSoundKind::Custom(id) => id,
        };
        let Some(entry) = self
            .session
            .saved_notification_sounds
            .iter()
            .find(|s| s.id == sound_id)
        else {
            return R::DefaultTone;
        };
        let file_id = entry.sound.id;
        if let Some(path) = self.session.file(file_id).and_then(|f| f.usable_path()) {
            return R::FilePath(path.into());
        }
        // Not local yet: mark the file as a notification sound, request
        // playback on completion, and start the download (deduped).
        self.session.sound_file_ids.insert(file_id.0, sound_id);
        self.session.pending_sound_downloads.insert(sound_id);
        // A play request is explicit: retry even if an earlier attempt
        // stalled (the stall mark only stops per-ingest auto retries).
        self.session.stalled_auto_downloads.remove(&file_id.0);
        let _ = self.download_file(file_id, USER_DOWNLOAD_PRIORITY);
        R::Pending
    }
}
