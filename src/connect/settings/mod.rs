//! Connect driver: privacy, notifications, sessions, storage, account.
use super::*;
use crate::data_settings::{AutoDownloadNetSettings, NetworkKind, save_data_storage_prefs};
use crate::ids::{ChatId, RequestId};
use crate::notify::NotificationSoundKind;
use crate::privacy::{PrivacyKeyState, PrivacyRuleDetail};
use crate::settings::{
    load_preferences, save_badge_prefs, save_call_prefs, save_device_prefs, save_language_prefs,
    save_preferences,
};
use crate::state::RequestPurpose;
use crate::state::SettingsPurpose;
use crate::telegram::envelope::{
    ChatNotificationSettings, MUTE_FOREVER, NotificationSettingsScope,
    ReactionNotificationSettings, ScopeNotificationSettings,
};
use crate::telegram::requests::{
    OptimizeStorage, delete_account, disconnect_all_websites, disconnect_website, get_account_ttl,
    get_active_sessions, get_chat_notification_settings_exceptions, get_connected_websites,
    get_saved_notification_sounds, get_scope_notification_settings, get_storage_statistics,
    optimize_storage, reset_all_notification_settings, set_account_ttl,
    set_chat_notification_settings, set_message_sender_block_list, set_option_boolean,
    set_option_integer, set_reaction_notification_settings, set_scope_notification_settings,
    terminate_all_other_sessions, terminate_session, toggle_session_can_accept_calls,
    toggle_session_can_accept_secret_chats,
};
use crate::telegram::requests_data_settings::{
    get_auto_download_settings_presets, set_auto_download_settings,
};
use crate::telegram::requests_privacy::{
    PrivacySettingKey, get_blocked_message_senders, get_privacy_rules,
    get_read_date_privacy_settings, set_privacy_rules, set_read_date_privacy_settings,
};

mod notifications;
mod sessions;
mod storage;

impl<S: JsonSender> ConnectDriver<S> {
    /// Slice S3: fetch one Privacy-screen rule
    /// (`getUserPrivacySettingRules`, schema 1.8.67, :15620).
    pub fn fetch_privacy_rules(&mut self, key: PrivacySettingKey) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session
            .settings
            .privacy
            .insert(key, PrivacyKeyState::Loading);
        let extra = self.session.request(
            RequestPurpose::Settings(SettingsPurpose::GetPrivacyRules { key }),
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&get_privacy_rules(extra, key.td_type()))
        {
            self.session.requests.take(extra);
            self.session
                .settings
                .privacy
                .insert(key, PrivacyKeyState::Failed);
            return Err(err);
        }
        Ok(())
    }

    /// Slice S3: change one Privacy-screen rule
    /// (`setUserPrivacySettingRules`, schema 1.8.67, :15617). Applied
    /// optimistically; the `ok` / error response confirms or fails it.
    pub fn set_privacy_rules(
        &mut self,
        key: PrivacySettingKey,
        detail: PrivacyRuleDetail,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::Settings(SettingsPurpose::SetPrivacyRules { key }),
            None,
        );
        let rules = detail.recompose();
        if let Err(err) = self
            .sender
            .send_json(&set_privacy_rules(extra, key.td_type(), rules))
        {
            self.session.requests.take(extra);
            self.session
                .settings
                .privacy
                .insert(key, PrivacyKeyState::Failed);
            return Err(err);
        }
        self.session.mirror_call_privacy(key, &detail);
        self.session
            .settings
            .privacy
            .insert(key, PrivacyKeyState::Ready(detail));
        Ok(extra)
    }

    /// Slice S3: fetch the read-date privacy setting
    /// (`getReadDatePrivacySettings`, schema 1.8.67, :15626).
    pub fn fetch_read_date_privacy(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.settings.read_date_loading = true;
        self.session.settings.read_date_error = false;
        let extra = self
            .session
            .request(RequestPurpose::GetReadDatePrivacy, None);
        if let Err(err) = self
            .sender
            .send_json(&get_read_date_privacy_settings(extra))
        {
            self.session.requests.take(extra);
            self.session.settings.read_date_loading = false;
            self.session.settings.read_date_error = true;
            return Err(err);
        }
        Ok(())
    }

    /// Slice S3: change the read-date privacy setting
    /// (`setReadDatePrivacySettings`, schema 1.8.67, :15623). Applied
    /// optimistically.
    pub fn set_read_date_privacy(&mut self, show: bool) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetReadDatePrivacy, None);
        if let Err(err) = self
            .sender
            .send_json(&set_read_date_privacy_settings(extra, show))
        {
            self.session.requests.take(extra);
            self.session.settings.read_date_loading = false;
            self.session.settings.read_date_error = true;
            return Err(err);
        }
        self.session.settings.read_date_show = Some(show);
        self.session.settings.read_date_loading = true;
        Ok(extra)
    }

    /// Slice S3: fetch the blocked-senders list
    /// (`getBlockedMessageSenders`, schema 1.8.67, :14505). Pages of
    /// 100; a later page continues from the loaded list length.
    pub fn fetch_blocked_senders(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let offset = self
            .session
            .settings
            .blocked_senders
            .as_ref()
            .map(|list| list.len())
            .unwrap_or(0);
        self.session.settings.blocked_loading = true;
        self.session.settings.blocked_error = false;
        let extra = self.session.request(
            RequestPurpose::Settings(SettingsPurpose::GetBlockedSenders {
                offset: offset as i32,
            }),
            None,
        );
        if let Err(err) =
            self.sender
                .send_json(&get_blocked_message_senders(extra, offset as i32, 100))
        {
            self.session.requests.take(extra);
            self.session.settings.blocked_loading = false;
            self.session.settings.blocked_error = true;
            return Err(err);
        }
        Ok(())
    }

    /// Slice S3: unblock a user (`setMessageSenderBlockList` with a null
    /// block list, schema 1.8.67, :14492 — TGX `Tdlib.unblockSender`).
    /// Applied optimistically: the user leaves the list at send time.
    pub fn unblock_sender(&mut self, user_id: i64) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::Settings(SettingsPurpose::SetSenderBlockList {
                user_id,
                block: false,
            }),
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&set_message_sender_block_list(extra, user_id, false))
        {
            self.session.requests.take(extra);
            self.session.settings.blocked_error = true;
            return Err(err);
        }
        if let Some(list) = self.session.settings.blocked_senders.as_mut() {
            list.retain(|id| *id != user_id);
            self.session.settings.blocked_total =
                self.session.settings.blocked_total.saturating_sub(1);
        }
        Ok(extra)
    }

    /// Slice S3: block a user (`setMessageSenderBlockList` with
    /// `blockListMain`, schema 1.8.67, :14492). Applied optimistically:
    /// the user joins the list at send time.
    pub fn block_sender(&mut self, user_id: i64) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::Settings(SettingsPurpose::SetSenderBlockList {
                user_id,
                block: true,
            }),
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&set_message_sender_block_list(extra, user_id, true))
        {
            self.session.requests.take(extra);
            self.session.settings.blocked_error = true;
            return Err(err);
        }
        let list = self
            .session
            .settings
            .blocked_senders
            .get_or_insert_with(Vec::new);
        if !list.contains(&user_id) {
            list.push(user_id);
            self.session.settings.blocked_total += 1;
        }
        Ok(extra)
    }
}

impl<S: JsonSender> ConnectDriver<S> {
    /// Phase C2i: persist the call preferences edited from the Calls
    /// tab (same account-scoped dir as the other settings files).
    pub fn save_call_prefs(&mut self) -> std::io::Result<()> {
        save_call_prefs(&self.paths, &self.session.calls.prefs)
    }

    /// MED1: persist media prefs (`media_prefs.json`) next to the account.
    pub fn save_media_prefs(&mut self) -> std::io::Result<()> {
        crate::settings::save_media_prefs(&self.paths, &self.session.settings.media_prefs)
    }

    /// Slice parity:chatlist-badge-settings: persist badge-counter prefs
    /// (`badge_prefs.json`) next to the account.
    pub fn save_badge_prefs(&mut self) -> std::io::Result<()> {
        save_badge_prefs(&self.paths, &self.session.settings.badge_prefs)
    }

    /// Parity slice: persist the in-app notification sounds toggle
    /// (`prefs.json`). Loads the existing prefs first so the other
    /// fields survive the write.
    pub fn save_inapp_sounds_enabled(&mut self) -> std::io::Result<()> {
        let mut prefs = load_preferences(&self.paths);
        prefs.inapp_sounds_enabled = self.session.settings.inapp_sounds_enabled;
        save_preferences(&self.paths, &prefs)
    }

    /// Persist the desktop-notifications switch (tdesktop `desktopNotify`).
    pub fn save_desktop_notifications(&mut self) -> std::io::Result<()> {
        let mut prefs = load_preferences(&self.paths);
        prefs.desktop_notifications = self.session.settings.desktop_notifications;
        save_preferences(&self.paths, &prefs)
    }

    /// Parity slice (platform-custom-keybindings): load the user's shortcut
    /// overrides (`prefs.json`).
    pub fn load_custom_keybindings(&self) -> Vec<crate::settings::CustomKeybinding> {
        load_preferences(&self.paths).custom_keybindings
    }

    /// Parity slice (platform-custom-keybindings): persist one shortcut
    /// override (`prefs.json`); an empty keystroke resets to the default.
    pub fn save_custom_keybinding(
        &mut self,
        custom: crate::settings::CustomKeybinding,
    ) -> std::io::Result<()> {
        let mut prefs = load_preferences(&self.paths);
        if custom.keystroke.is_empty() {
            prefs.custom_keybindings.retain(|c| c.id != custom.id);
        } else if let Some(existing) = prefs
            .custom_keybindings
            .iter_mut()
            .find(|c| c.id == custom.id)
        {
            existing.keystroke = custom.keystroke;
        } else {
            prefs.custom_keybindings.push(custom);
        }
        save_preferences(&self.paths, &prefs)
    }

    /// Slice parity:settings-language: persist the app language pref
    /// (`language_prefs.json`) next to the account.
    pub fn save_language_prefs(&mut self) -> std::io::Result<()> {
        save_language_prefs(&self.paths, &self.session.settings.language_prefs)
    }

    /// parity:settings-session-details: persist the custom device name.
    pub fn save_device_prefs(&mut self) -> std::io::Result<()> {
        save_device_prefs(&self.paths, &self.session.settings.device_prefs)
    }
}
