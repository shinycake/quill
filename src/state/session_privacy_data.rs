//! B13: reducer state for the privacy / security / data settings that sit
//! next to the rule screen: new-chat privacy ("Who can message me"), the
//! inactive-session TTL, the 18+ content option, network usage, and the
//! "Do you still remember your password?" check.
use super::*;
use crate::network_usage::NetworkUsage;
use crate::privacy::{NewChatPrivacy, NewChatPrivacyState};

/// Outcome of the "Do you still remember your password?" check.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PasswordCheck {
    #[default]
    Idle,
    Checking,
    Remembered,
    Wrong,
    Failed,
}

#[derive(Debug, Clone, Default)]
pub struct PrivacyData {
    /// `getNewChatPrivacySettings` state; `None` = never asked.
    pub new_chat: Option<NewChatPrivacyState>,
    /// Days of inactivity before sessions end (`getActiveSessions`).
    pub inactive_session_ttl_days: Option<i32>,
    /// `ignore_sensitive_content_restrictions` as TDLib last reported it.
    pub ignore_sensitive: Option<bool>,
    /// `can_ignore_sensitive_content_restrictions`: the server lets this
    /// account turn the 18+ toggle on.
    pub can_ignore_sensitive: bool,
    /// `getNetworkStatistics` answer.
    pub network_usage: Option<NetworkUsage>,
    pub network_loading: bool,
    /// The server asked the client to check the password
    /// (`suggestedActionCheckPassword`).
    pub check_password_suggested: bool,
    pub password_check: PasswordCheck,
    /// Last failure of one of the requests above, shown where it was
    /// triggered (never as a toast).
    pub error: Option<String>,
}

impl Session {
    /// Fold the options this block reads.
    pub(crate) fn apply_privacy_option(&mut self, name: &str, value: &OptionValue) {
        match (name, value) {
            ("ignore_sensitive_content_restrictions", OptionValue::Boolean(on)) => {
                self.privacy_data.ignore_sensitive = Some(*on);
            }
            ("can_ignore_sensitive_content_restrictions", OptionValue::Boolean(on)) => {
                self.privacy_data.can_ignore_sensitive = *on;
            }
            _ => {}
        }
    }

    /// Payloads of this block. Returns true when handled.
    pub(crate) fn apply_privacy_data_payload(
        &mut self,
        payload: &EnvelopePayload,
        pending: Option<&PendingRequest>,
    ) -> bool {
        let purpose = pending.map(|p| p.purpose);
        match payload {
            EnvelopePayload::NewChatPrivacySettings(settings) => {
                if purpose == Some(RequestPurpose::GetNewChatPrivacy) {
                    self.privacy_data.new_chat = Some(NewChatPrivacyState::Ready(*settings));
                }
                true
            }
            EnvelopePayload::NetworkStatistics(usage) => {
                if purpose == Some(RequestPurpose::GetNetworkStatistics) {
                    self.privacy_data.network_usage = Some(usage.clone());
                    self.privacy_data.network_loading = false;
                    self.privacy_data.error = None;
                }
                true
            }
            EnvelopePayload::RecoveryEmailAddress => {
                if purpose == Some(RequestPurpose::CheckRememberedPassword) {
                    // The finish step shows until the user taps Done, which
                    // sends `hideSuggestedAction`.
                    self.privacy_data.password_check = PasswordCheck::Remembered;
                }
                true
            }
            EnvelopePayload::UpdateSuggestedActions { added, removed } => {
                for name in added {
                    self.suggestions.actions.insert(name.clone());
                }
                for name in removed {
                    self.suggestions.actions.remove(name);
                }
                const CHECK: &str = "suggestedActionCheckPassword";
                if added.iter().any(|name| name == CHECK) {
                    self.privacy_data.check_password_suggested = true;
                }
                if removed.iter().any(|name| name == CHECK) {
                    self.privacy_data.check_password_suggested = false;
                }
                true
            }
            _ => false,
        }
    }

    /// `ok` of this block's purposes.
    pub(crate) fn apply_privacy_data_ok(&mut self, pending: Option<&PendingRequest>) {
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::ResetNetworkStatistics) => {
                // The old numbers are gone; the next open refetches.
                self.privacy_data.network_usage = None;
                self.privacy_data.error = None;
            }
            Some(RequestPurpose::HideCheckPasswordSuggestion) => {
                self.privacy_data.check_password_suggested = false;
            }
            Some(RequestPurpose::SetInactiveSessionTtl) => self.privacy_data.error = None,
            _ => {}
        }
    }

    /// Failure of this block's purposes.
    pub(crate) fn apply_privacy_data_error(&mut self, purpose: RequestPurpose, err: &TdError) {
        match purpose {
            RequestPurpose::HideSuggestedAction { action } => {
                self.suggestions.actions.insert(action.to_string());
                self.chat_action_error = Some(format!(
                    "could not hide the suggestion (error {})",
                    err.code
                ));
                return;
            }
            RequestPurpose::HideContactCloseBirthdays => {
                self.suggestions.birthdays_hidden = false;
                self.chat_action_error = Some(format!(
                    "could not hide the suggestion (error {})",
                    err.code
                ));
                return;
            }
            _ => {}
        }
        let data = &mut self.privacy_data;
        match purpose {
            RequestPurpose::GetNewChatPrivacy => {
                data.new_chat = Some(NewChatPrivacyState::Failed);
            }
            RequestPurpose::SetNewChatPrivacy { previous_allow } => {
                // Roll the optimistic choice back to what the server had.
                if let Some(NewChatPrivacyState::Ready(current)) = data.new_chat {
                    data.new_chat = Some(NewChatPrivacyState::Ready(NewChatPrivacy {
                        allow_from_unknown: previous_allow,
                        ..current
                    }));
                }
                data.error = Some(sessions_error_line("change who can message you", err));
            }
            RequestPurpose::SetGiftSettings => {
                data.error = Some(sessions_error_line("change the gift settings", err));
            }
            RequestPurpose::SetInactiveSessionTtl => {
                data.inactive_session_ttl_days = None;
                data.error = Some(sessions_error_line("change the session timeout", err));
            }
            RequestPurpose::SetSensitiveContent => {
                data.error = Some(sessions_error_line("change the 18+ setting", err));
            }
            RequestPurpose::SetContactJoinedNotifications => {
                data.error = Some(sessions_error_line(
                    "change the contact-joined notifications",
                    err,
                ));
            }
            RequestPurpose::GetNetworkStatistics => {
                data.network_loading = false;
                data.error = Some(sessions_error_line("load the network usage", err));
            }
            RequestPurpose::ResetNetworkStatistics => {
                data.error = Some(sessions_error_line("reset the network usage", err));
            }
            RequestPurpose::CheckRememberedPassword => {
                data.password_check = if err.class == ErrorClass::Invalid {
                    PasswordCheck::Wrong
                } else {
                    PasswordCheck::Failed
                };
            }
            _ => {}
        }
    }

    /// Optimistic write of the new-chat row (kept when the answer is `ok`).
    pub fn set_new_chat_privacy_local(&mut self, settings: NewChatPrivacy) {
        self.privacy_data.new_chat = Some(NewChatPrivacyState::Ready(settings));
    }

    /// The own user's gift settings once `userFullInfo` was seen.
    pub fn my_gift_settings(&self) -> Option<crate::privacy::GiftSettings> {
        let me = self.my_user_id?;
        self.user_full_infos.get(&me)?.extras.gift_settings
    }

    /// Optimistic write of the own gift settings.
    pub fn set_my_gift_settings_local(&mut self, settings: crate::privacy::GiftSettings) {
        if let Some(me) = self.my_user_id
            && let Some(info) = self.user_full_infos.get_mut(&me)
        {
            info.extras.gift_settings = Some(settings);
        }
    }
}

impl Session {
    /// The two call rules also feed `call_privacy_allow_calls` /
    /// `call_privacy_p2p`, which the older call code reads. Called
    /// whenever the rule detail for a call key changes.
    pub fn mirror_call_privacy(&mut self, key: PrivacySettingKey, detail: &PrivacyRuleDetail) {
        match key {
            PrivacySettingKey::AllowCalls => self.call_privacy_allow_calls = detail.who,
            PrivacySettingKey::PeerToPeer => self.call_privacy_p2p = detail.who,
            _ => {}
        }
    }
}
