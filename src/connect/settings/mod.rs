//! Connect driver: privacy, notifications, sessions, storage, account.
use super::*;
use crate::data_settings::{AutoDownloadNetSettings, NetworkKind, Seed, save_data_storage_prefs};
use crate::ids::{ChatId, RequestId};
use crate::notify::NotificationSoundKind;
use crate::privacy::{PrivacyKeyState, PrivacyRuleDetail};
use crate::settings::{
    load_preferences, save_badge_prefs, save_call_prefs, save_device_prefs, save_language_prefs,
    save_preferences,
};
use crate::state::BlockOrigin;
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
pub(super) use note_send::NoteSlot;

mod note_send;
mod notifications;
mod sessions;
mod storage;
mod tone_upload;

impl<S: JsonSender> ConnectDriver<S> {
    /// Slice S3: fetch one Privacy-screen rule
    /// (`getUserPrivacySettingRules`, schema 1.8.67, :15620).
    pub fn fetch_privacy_rules(&mut self, key: PrivacySettingKey) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            self.session.settings.refuse_privacy_load(key);
            return Err(ConnectSendError::InvalidRequest);
        }
        let busy = self
            .session
            .settings
            .privacy
            .get(&key)
            .is_some_and(|state| {
                matches!(
                    state,
                    PrivacyKeyState::Loading { .. } | PrivacyKeyState::Saving { .. }
                )
            });
        if busy {
            return Ok(());
        }
        let extra = self.session.request(
            RequestPurpose::Settings(SettingsPurpose::GetPrivacyRules { key }),
            None,
        );
        self.session.settings.begin_privacy_load(key, extra);
        if let Err(err) = self
            .sender
            .send_json(&get_privacy_rules(extra, key.td_type()))
        {
            self.session.requests.take(extra);
            self.session.settings.fail_privacy_load(key, Some(extra));
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
            return Err(err);
        }
        self.session.settings.begin_privacy_save(key, detail, extra);
        Ok(extra)
    }

    /// Slice S3: fetch the read-date privacy setting
    /// (`getReadDatePrivacySettings`, schema 1.8.67, :15626).
    pub fn fetch_read_date_privacy(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        // A set in flight already holds the value the server will have;
        // a get answered after it would undo the user's change.
        if self.session.settings.read_date.setting() {
            return Ok(());
        }
        let extra = self
            .session
            .request(RequestPurpose::GetReadDatePrivacy, None);
        if let Err(err) = self
            .sender
            .send_json(&get_read_date_privacy_settings(extra))
        {
            self.session.requests.take(extra);
            self.session.settings.read_date.send_refused();
            return Err(err);
        }
        self.session.settings.read_date.start_get(extra);
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
            self.session.settings.read_date.send_refused();
            return Err(err);
        }
        // A get still in flight is superseded: its answer no longer lands.
        self.session.settings.read_date.start_set(extra, show);
        Ok(extra)
    }

    /// Slice S3: fetch the blocked-senders list from its first page
    /// (`getBlockedMessageSenders`, schema 1.8.67, :14505). What the privacy
    /// page does on every open: the loaded list stays on screen and the
    /// first page replaces it when it arrives, so a user blocked on another
    /// device shows up. Pages of 100.
    pub fn fetch_blocked_senders(&mut self) -> Result<(), ConnectSendError> {
        self.ask_blocked_page(0)
    }

    /// The next page of the blocked-senders list, continuing from the
    /// loaded length (the first page when nothing is loaded).
    pub fn fetch_more_blocked_senders(&mut self) -> Result<(), ConnectSendError> {
        let offset = self.session.settings.blocked_ids().len();
        self.ask_blocked_page(offset)
    }

    fn ask_blocked_page(&mut self, offset: usize) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        // A next page waits for the one out; a first page replaces it, so
        // the page asked before no longer matches.
        if offset > 0 && self.session.settings.blocked_senders.is_loading() {
            return Ok(());
        }
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
            let blocked = &mut self.session.settings.blocked_senders;
            if !blocked.is_loading() {
                blocked.fail("The blocked users request was not sent.");
            }
            return Err(err);
        }
        // Only a page that went out is waited for, under its own request.
        self.session.settings.blocked_senders.begin_with(extra);
        Ok(())
    }

    /// The first page again once a block or unblock that replaced a page in
    /// flight has landed (the list is `Outdated`).
    pub(crate) fn refetch_blocked_senders(&mut self) -> Result<(), ConnectSendError> {
        if self.session.settings.blocked_senders.is_outdated() {
            self.ask_blocked_page(0)?;
        }
        Ok(())
    }

    /// Slice S3: unblock a user (`setMessageSenderBlockList` with a null
    /// block list, schema 1.8.67, :14492 — TGX `Tdlib.unblockSender`).
    /// Applied optimistically: the user leaves the list at send time. Only
    /// the Privacy page unblocks, so its note waits for the answer.
    pub fn unblock_sender(&mut self, user_id: i64) -> Result<RequestId, ConnectSendError> {
        self.change_block_list(user_id, false, BlockOrigin::PrivacyPage)
    }

    /// Slice S3: block a user (`setMessageSenderBlockList` with
    /// `blockListMain`, schema 1.8.67, :14492). Applied optimistically:
    /// the user joins the list at send time. `origin` says who asked: only
    /// the Privacy page's own block starts the block-list note's wait.
    pub fn block_sender(
        &mut self,
        user_id: i64,
        origin: BlockOrigin,
    ) -> Result<RequestId, ConnectSendError> {
        self.change_block_list(user_id, true, origin)
    }

    fn change_block_list(
        &mut self,
        user_id: i64,
        block: bool,
        origin: BlockOrigin,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::Settings(SettingsPurpose::SetSenderBlockList { user_id, block }),
            None,
        );
        let json = set_message_sender_block_list(extra, user_id, block);
        if origin == BlockOrigin::PrivacyPage {
            let refusal = if block {
                "The block request was not sent."
            } else {
                "The unblock request was not sent."
            };
            self.send_noted(NoteSlot::Block, extra, refusal, &json)?;
        } else if let Err(err) = self.sender.send_json(&json) {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session
            .settings
            .block_change_sent(extra, user_id, block);
        Ok(extra)
    }
}

impl<S: JsonSender> ConnectDriver<S> {
    /// Phase C2i: persist the call preferences edited from the Calls
    /// tab (same account-scoped dir as the other settings files).
    pub fn save_call_prefs(&mut self) {
        save_call_prefs(&self.paths, &self.session.calls.prefs)
    }

    /// MED1: persist media prefs (`media_prefs.json`) next to the account.
    pub fn save_media_prefs(&mut self) {
        crate::settings::save_media_prefs(&self.paths, &self.session.settings.media_prefs)
    }

    /// Slice parity:chatlist-badge-settings: persist badge-counter prefs
    /// (`badge_prefs.json`) next to the account.
    pub fn save_badge_prefs(&mut self) -> std::io::Result<()> {
        save_badge_prefs(&self.paths, &self.session.settings.badge_prefs)
    }

    /// Save `prefs` and, once they are on disk, apply them to the session:
    /// a failed save leaves the value the switch shows as it was.
    pub fn set_badge_prefs(&mut self, prefs: crate::settings::BadgePrefs) -> std::io::Result<()> {
        save_badge_prefs(&self.paths, &prefs)?;
        self.session.settings.badge_prefs = prefs;
        Ok(())
    }

    /// Parity slice: persist the in-app notification sounds toggle
    /// (`prefs.json`). Loads the existing prefs first so the other
    /// fields survive the write.
    pub fn save_inapp_sounds_enabled(&mut self) {
        let mut prefs = load_preferences(&self.paths);
        prefs.inapp_sounds_enabled = self.session.settings.inapp_sounds_enabled;
        save_preferences(&self.paths, &prefs)
    }

    /// Persist the notification view (tdesktop "Name" / "Text") to
    /// `prefs.json`, keeping the other prefs.
    pub fn save_notify_view(&mut self) {
        let mut prefs = load_preferences(&self.paths);
        prefs.notify_view = self.session.settings.notify_view;
        save_preferences(&self.paths, &prefs)
    }

    /// Persist "Show photo previews" to `prefs.json`, keeping the other
    /// prefs.
    pub fn save_notify_photo_previews(&mut self) {
        let mut prefs = load_preferences(&self.paths);
        prefs.notify_photo_previews = self.session.settings.notify_photo_previews;
        save_preferences(&self.paths, &prefs)
    }

    /// Parity slice (platform-custom-keybindings): load the user's shortcut
    /// overrides (`prefs.json`).
    pub fn load_custom_keybindings(&self) -> Vec<crate::settings::CustomKeybinding> {
        load_preferences(&self.paths).custom_keybindings
    }

    /// Parity slice (platform-custom-keybindings): persist one shortcut
    /// override (`prefs.json`); an empty keystroke resets to the default.
    pub fn save_custom_keybinding(&mut self, custom: crate::settings::CustomKeybinding) {
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
    pub fn save_language_prefs(&mut self) {
        save_language_prefs(&self.paths, &self.session.settings.language_prefs)
    }

    /// parity:settings-session-details: persist the custom device name.
    pub fn save_device_prefs(&mut self) {
        save_device_prefs(&self.paths, &self.session.settings.device_prefs)
    }
}
