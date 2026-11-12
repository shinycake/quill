//! Connect driver: B13 privacy / security / data requests — new-chat
//! privacy, gift settings, the inactive-session TTL, the 18+ option,
//! network usage and the remember-password check.
use super::*;
use crate::ids::RequestId;
use crate::privacy::{GiftSettings, NewChatPrivacy, NewChatPrivacyState};
use crate::state::RequestPurpose;
use crate::state::SettingsPurpose;
use crate::telegram::requests::set_option_boolean;
use crate::telegram::requests_privacy::{
    get_network_statistics, get_new_chat_privacy_settings, get_recovery_email_address,
    hide_check_password_suggestion, hide_contact_close_birthdays, hide_suggested_action,
    reset_network_statistics, set_gift_settings, set_inactive_session_ttl,
    set_new_chat_privacy_settings,
};

/// TDLib accepts 1-366 days (`setInactiveSessionTtl`, schema 1.8.67).
pub const MAX_INACTIVE_SESSION_DAYS: i32 = 366;

impl<S: JsonSender> ConnectDriver<S> {
    /// `getNewChatPrivacySettings` — the "Who can message me" row.
    pub fn fetch_new_chat_privacy(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        // One fetch at a time: the first is the stage's own, a refresh over a
        // value already shown holds its request in `new_chat_refresh`, stays
        // quiet and keeps taking saves.
        let data = &self.session.settings.privacy_data;
        if matches!(data.new_chat, Some(NewChatPrivacyState::Loading(_)))
            || data.new_chat_refresh.is_some()
        {
            return Ok(());
        }
        let extra = self
            .session
            .request(RequestPurpose::GetNewChatPrivacy, None);
        let showing = self
            .session
            .settings
            .privacy_data
            .new_chat
            .as_ref()
            .is_some_and(|state| state.value().is_some());
        if !showing {
            self.session.settings.privacy_data.new_chat = Some(NewChatPrivacyState::Loading(extra));
        }
        if let Err(err) = self.sender.send_json(&get_new_chat_privacy_settings(extra)) {
            self.session.requests.take(extra);
            if !showing {
                self.session.settings.privacy_data.new_chat = Some(NewChatPrivacyState::Failed);
            }
            return Err(err);
        }
        if showing {
            self.session.settings.privacy_data.new_chat_refresh = Some(extra);
        }
        Ok(())
    }

    /// The value the "Who can message me" row shows, once it is known.
    fn new_chat_value(&self) -> Option<NewChatPrivacy> {
        self.session
            .settings
            .privacy_data
            .new_chat
            .as_ref()
            .and_then(NewChatPrivacyState::value)
    }

    /// `setNewChatPrivacySettings`: Everyone (`true`) or Contacts and
    /// Premium users (`false`). The paid-message price is sent back
    /// unchanged. Applied optimistically.
    pub fn set_new_chat_privacy(
        &mut self,
        allow_from_unknown: bool,
    ) -> Result<RequestId, ConnectSendError> {
        let Some(current) = self.new_chat_value() else {
            // Not loaded: writing would reset the paid-message price.
            return Err(ConnectSendError::InvalidRequest);
        };
        self.set_new_chat_settings(NewChatPrivacy {
            allow_from_unknown,
            ..current
        })
    }

    /// `setNewChatPrivacySettings` with the whole value: the Messages box
    /// saves Everybody, Contacts and Premium, or a price in one request.
    /// Applied optimistically; a failure restores the previous value.
    pub fn set_new_chat_settings(
        &mut self,
        next: NewChatPrivacy,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.new_chat_value().is_none() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::Settings(SettingsPurpose::SetNewChatPrivacy),
            None,
        );
        if let Err(err) = self.sender.send_json(&set_new_chat_privacy_settings(
            extra,
            next.allow_from_unknown,
            next.incoming_paid_message_star_count,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        if let Some(state) = self.session.settings.privacy_data.new_chat.as_mut() {
            state.save(extra, next);
        }
        Ok(extra)
    }

    /// `setGiftSettings`. Applied optimistically to the own full info.
    pub fn set_gift_settings(
        &mut self,
        settings: GiftSettings,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(RequestPurpose::SetGiftSettings, None);
        if let Err(err) = self
            .sender
            .send_json(&set_gift_settings(extra, settings.to_value()))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        // The value on screen is what a refusal returns to; without one
        // (the full info never arrived) the write below is a no-op too.
        if let Some(shown) = self.session.my_gift_settings() {
            self.session
                .settings
                .privacy_data
                .gift_save
                .save(shown, extra, settings);
        }
        self.session.set_my_gift_settings_local(settings);
        Ok(extra)
    }

    /// `setInactiveSessionTtl`. Applied optimistically.
    pub fn set_inactive_session_ttl(&mut self, days: i32) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || !(1..=MAX_INACTIVE_SESSION_DAYS).contains(&days) {
            return Err(ConnectSendError::InvalidRequest);
        }
        // The value the server holds is the one to fall back to: without
        // it nothing goes out.
        if self
            .session
            .settings
            .privacy_data
            .inactive_session_ttl
            .value()
            .is_none()
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::Settings(SettingsPurpose::SetInactiveSessionTtl),
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&set_inactive_session_ttl(extra, days))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session
            .settings
            .privacy_data
            .inactive_session_ttl
            .save(extra, days);
        self.session
            .settings
            .privacy_data
            .clear_error(crate::state::PrivacyControl::SessionTtl);
        Ok(extra)
    }

    /// Settings > Privacy > "Show 18+ content":
    /// `setOption(ignore_sensitive_content_restrictions)`. TDLib answers
    /// with `updateOption`, which is the only thing that moves the switch.
    pub fn set_ignore_sensitive_content(
        &mut self,
        on: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || !self.session.settings.privacy_data.can_ignore_sensitive {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetSensitiveContent, None);
        if let Err(err) = self.sender.send_json(&set_option_boolean(
            extra,
            "ignore_sensitive_content_restrictions",
            on,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session
            .settings
            .privacy_data
            .clear_error(crate::state::PrivacyControl::Sensitive);
        Ok(extra)
    }

    /// Settings > Notifications > Events > "Contact joined Telegram":
    /// `setOption(disable_contact_registered_notifications)`. TDLib answers
    /// with `updateOption`, which moves the switch.
    pub fn set_contact_joined_notifications(
        &mut self,
        on: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetContactJoinedNotifications, None);
        if let Err(err) = self.sender.send_json(&set_option_boolean(
            extra,
            "disable_contact_registered_notifications",
            !on,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session
            .settings
            .privacy_data
            .clear_error(crate::state::PrivacyControl::ContactJoined);
        Ok(extra)
    }

    /// `getNetworkStatistics` (everything since the last reset).
    pub fn fetch_network_statistics(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetNetworkStatistics, None);
        if let Err(err) = self.sender.send_json(&get_network_statistics(extra)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        // Only an ask that went out is waited for, under its own request.
        let network = &mut self.session.settings.privacy_data.network;
        network.begin();
        network.sent(extra);
        Ok(())
    }

    /// `resetNetworkStatistics`, then a refetch for the new start date.
    pub fn reset_network_statistics(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::ResetNetworkStatistics, None);
        if let Err(err) = self.sender.send_json(&reset_network_statistics(extra)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session.settings.privacy_data.network = Default::default();
        let _ = self.fetch_network_statistics();
        Ok(extra)
    }

    /// "Do you still remember your password?": `getRecoveryEmailAddress`
    /// accepts only the right password. The caller zeroizes `password`.
    pub fn check_remembered_password(&mut self, password: &str) -> Result<(), ConnectSendError> {
        if password.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.chats_path_active() {
            // The card says the check failed, rather than staying as it was.
            self.session.settings.privacy_data.password_check = crate::state::PasswordCheck::Failed;
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::CheckRememberedPassword, None);
        self.session.settings.privacy_data.password_check =
            crate::state::PasswordCheck::Checking { request: extra };
        if let Err(err) = self
            .sender
            .send_json(&get_recovery_email_address(extra, password))
        {
            self.session.requests.take(extra);
            self.session.settings.privacy_data.password_check = crate::state::PasswordCheck::Failed;
            return Err(err);
        }
        Ok(())
    }

    /// Dismiss the chat-list suggestion. The block goes at once; a refusal
    /// brings it back.
    pub fn hide_suggestion(
        &mut self,
        suggestion: &crate::chatlist_suggestions::Suggestion,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        match crate::chatlist_suggestions::dismiss_action(suggestion) {
            Some(action) => self.hide_suggested_action(action),
            None => {
                let extra = self
                    .session
                    .request(RequestPurpose::HideContactCloseBirthdays, None);
                if let Err(err) = self.sender.send_json(&hide_contact_close_birthdays(extra)) {
                    self.session.requests.take(extra);
                    return Err(err);
                }
                self.session.chat_list.suggestions.hide_birthdays(extra);
                Ok(extra)
            }
        }
    }

    /// `hideSuggestedAction(action)`. The action goes at once; a refusal
    /// brings it back.
    pub fn hide_suggested_action(
        &mut self,
        action: &'static str,
    ) -> Result<RequestId, ConnectSendError> {
        // The user said no: the suggestion is gone here whether or not the
        // server can be told (a closing connection), so a box that offers it
        // does not open again and again.
        self.session.chat_list.suggestions.remove_action(action);
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::Settings(SettingsPurpose::HideSuggestedAction { action }),
            None,
        );
        if let Err(err) = self.sender.send_json(&hide_suggested_action(extra, action)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Dismiss the password check (`hideSuggestedAction`).
    pub fn hide_check_password_suggestion(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::HideCheckPasswordSuggestion, None);
        if let Err(err) = self
            .sender
            .send_json(&hide_check_password_suggestion(extra))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        // The card goes only once the request left: a refused send keeps
        // it (and a check in flight), so the caller can say so.
        self.session
            .chat_list
            .suggestions
            .remove_action(crate::chatlist_suggestions::ACTION_PASSWORD);
        self.session.settings.privacy_data.password_check = crate::state::PasswordCheck::Idle;
        Ok(extra)
    }
}
