//! Connect driver: B13 privacy / security / data requests — new-chat
//! privacy, gift settings, the inactive-session TTL, the 18+ option,
//! network usage and the remember-password check.
use super::*;
use crate::ids::RequestId;
use crate::privacy::{GiftSettings, NewChatPrivacy, NewChatPrivacyState};
use crate::state::RequestPurpose;
use crate::telegram::requests::set_option_boolean;
use crate::telegram::requests_privacy::{
    get_network_statistics, get_new_chat_privacy_settings, get_recovery_email_address,
    hide_check_password_suggestion, reset_network_statistics, set_gift_settings,
    set_inactive_session_ttl, set_new_chat_privacy_settings,
};

/// TDLib accepts 1-366 days (`setInactiveSessionTtl`, schema 1.8.67).
pub const MAX_INACTIVE_SESSION_DAYS: i32 = 366;

impl<S: JsonSender> ConnectDriver<S> {
    /// `getNewChatPrivacySettings` — the "Who can message me" row.
    pub fn fetch_new_chat_privacy(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.privacy_data.new_chat = Some(NewChatPrivacyState::Loading);
        let extra = self
            .session
            .request(RequestPurpose::GetNewChatPrivacy, None);
        if let Err(err) = self.sender.send_json(&get_new_chat_privacy_settings(extra)) {
            self.session.requests.take(extra);
            self.session.privacy_data.new_chat = Some(NewChatPrivacyState::Failed);
            return Err(err);
        }
        Ok(())
    }

    /// `setNewChatPrivacySettings`: Everyone (`true`) or Contacts and
    /// Premium users (`false`). The paid-message price is sent back
    /// unchanged. Applied optimistically.
    pub fn set_new_chat_privacy(
        &mut self,
        allow_from_unknown: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(NewChatPrivacyState::Ready(current)) = self.session.privacy_data.new_chat else {
            // Not loaded: writing would reset the paid-message price.
            return Err(ConnectSendError::InvalidRequest);
        };
        let next = NewChatPrivacy {
            allow_from_unknown,
            ..current
        };
        let extra = self.session.request(
            RequestPurpose::SetNewChatPrivacy {
                previous_allow: current.allow_from_unknown,
            },
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
        self.session.set_new_chat_privacy_local(next);
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
        self.session.set_my_gift_settings_local(settings);
        Ok(extra)
    }

    /// `setInactiveSessionTtl`. Applied optimistically.
    pub fn set_inactive_session_ttl(&mut self, days: i32) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || !(1..=MAX_INACTIVE_SESSION_DAYS).contains(&days) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetInactiveSessionTtl, None);
        if let Err(err) = self
            .sender
            .send_json(&set_inactive_session_ttl(extra, days))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session.privacy_data.inactive_session_ttl_days = Some(days);
        self.session.privacy_data.error = None;
        Ok(extra)
    }

    /// Settings > Privacy > "Show 18+ content":
    /// `setOption(ignore_sensitive_content_restrictions)`. TDLib answers
    /// with `updateOption`, which is the only thing that moves the switch.
    pub fn set_ignore_sensitive_content(
        &mut self,
        on: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || !self.session.privacy_data.can_ignore_sensitive {
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
        self.session.privacy_data.error = None;
        Ok(extra)
    }

    /// `getNetworkStatistics` (everything since the last reset).
    pub fn fetch_network_statistics(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.privacy_data.network_loading = true;
        let extra = self
            .session
            .request(RequestPurpose::GetNetworkStatistics, None);
        if let Err(err) = self.sender.send_json(&get_network_statistics(extra)) {
            self.session.requests.take(extra);
            self.session.privacy_data.network_loading = false;
            return Err(err);
        }
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
        self.session.privacy_data.network_usage = None;
        let _ = self.fetch_network_statistics();
        Ok(extra)
    }

    /// "Do you still remember your password?": `getRecoveryEmailAddress`
    /// accepts only the right password. The caller zeroizes `password`.
    pub fn check_remembered_password(&mut self, password: &str) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() || password.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.privacy_data.password_check = crate::state::PasswordCheck::Checking;
        let extra = self
            .session
            .request(RequestPurpose::CheckRememberedPassword, None);
        if let Err(err) = self
            .sender
            .send_json(&get_recovery_email_address(extra, password))
        {
            self.session.requests.take(extra);
            self.session.privacy_data.password_check = crate::state::PasswordCheck::Failed;
            return Err(err);
        }
        Ok(())
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
        self.session.privacy_data.check_password_suggested = false;
        Ok(extra)
    }
}
