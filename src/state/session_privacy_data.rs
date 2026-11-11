//! B13: reducer state for the privacy / security / data settings that sit
//! next to the rule screen: new-chat privacy ("Who can message me"), the
//! inactive-session TTL, the 18+ content option, network usage, and the
//! "Do you still remember your password?" check.
use super::*;
use crate::network_usage::NetworkUsage;
use crate::privacy::{NewChatPrivacy, NewChatPrivacyState};
use crate::telegram::envelope::SettingsPayload;

/// The controls of this block, each with a line of its own for the last
/// request of it that failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PrivacyControl {
    NewChat,
    Gifts,
    SessionTtl,
    Sensitive,
    ContactJoined,
}

/// Outcome of the "Do you still remember your password?" check.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PasswordCheck {
    #[default]
    Idle,
    /// `getRecoveryEmailAddress` is out for the typed password; only its
    /// answer settles the check, and dismissing the box drops it.
    Checking {
        request: RequestId,
    },
    Remembered,
    Wrong,
    Failed,
}

impl PasswordCheck {
    /// The check is out for this very request.
    pub fn waits_for(self, pending: Option<&PendingRequest>) -> bool {
        matches!(self, Self::Checking { request } if Some(request) == pending.map(|p| p.id))
    }

    pub fn is_checking(self) -> bool {
        matches!(self, Self::Checking { .. })
    }
}

#[derive(Debug, Clone, Default)]
pub struct PrivacyData {
    /// `getNewChatPrivacySettings` state; `None` = never asked.
    pub new_chat: Option<NewChatPrivacyState>,
    /// The newest save of `new_chat`. A fetch asked before it answers with
    /// the older server value and is dropped.
    pub new_chat_saved_at: Option<RequestId>,
    /// Days of inactivity before sessions end (`getActiveSessions`).
    pub inactive_session_ttl_days: Option<i32>,
    /// `ignore_sensitive_content_restrictions` as TDLib last reported it.
    pub ignore_sensitive: Option<bool>,
    /// `can_ignore_sensitive_content_restrictions`: the server lets this
    /// account turn the 18+ toggle on.
    pub can_ignore_sensitive: bool,
    /// `getNetworkStatistics`: the answer, a refetch under it, or why it
    /// failed (a failed reset lands here too).
    pub network: crate::state::fetch::Fetch<NetworkUsage>,
    /// The server asked the client to check the password
    /// (`suggestedActionCheckPassword`).
    pub check_password_suggested: bool,
    pub password_check: PasswordCheck,
    /// The last failure of each control's request, shown where that
    /// control is (never as a toast). A control's success or retry clears
    /// only its own line.
    pub errors: std::collections::BTreeMap<PrivacyControl, String>,
}

impl PrivacyData {
    pub fn error(&self, control: PrivacyControl) -> Option<&str> {
        self.errors.get(&control).map(String::as_str)
    }

    pub fn clear_error(&mut self, control: PrivacyControl) {
        self.errors.remove(&control);
    }

    fn fail(&mut self, control: PrivacyControl, line: String) {
        self.errors.insert(control, line);
    }
}

impl Session {
    /// Fold the options this block reads.
    pub(crate) fn apply_privacy_option(&mut self, name: &str, value: &OptionValue) {
        match (name, value) {
            ("ignore_sensitive_content_restrictions", OptionValue::Boolean(on)) => {
                self.settings.privacy_data.ignore_sensitive = Some(*on);
            }
            ("can_ignore_sensitive_content_restrictions", OptionValue::Boolean(on)) => {
                self.settings.privacy_data.can_ignore_sensitive = *on;
            }
            _ => {}
        }
        self.payments.paid_messages.apply_option(name, value);
    }

    /// Payloads of this block. Returns true when handled.
    pub(crate) fn apply_privacy_data_payload(
        &mut self,
        payload: &SettingsPayload,
        pending: Option<&PendingRequest>,
    ) -> bool {
        let purpose = pending.map(|p| p.purpose);
        match payload {
            SettingsPayload::NewChatPrivacySettings(settings) => {
                if purpose == Some(RequestPurpose::GetNewChatPrivacy)
                    && let Some(id) = pending.map(|p| p.id)
                {
                    let data = &mut self.settings.privacy_data;
                    let first = matches!(data.new_chat, Some(NewChatPrivacyState::Loading(live)) if live == id);
                    let newer_than_save = data.new_chat_saved_at.is_none_or(|saved| id.0 > saved.0);
                    if first
                        || (newer_than_save
                            && matches!(data.new_chat, Some(NewChatPrivacyState::Ready(_))))
                    {
                        data.new_chat = Some(NewChatPrivacyState::Ready(*settings));
                    }
                }
                true
            }
            SettingsPayload::CanSendMessageToUser(answer) => {
                if purpose == Some(RequestPurpose::CanSendMessageToUser)
                    && let Some(user_id) = pending.and_then(|p| p.user_id)
                {
                    let premium = self.my_is_premium();
                    self.payments.paid_messages.record_answer(
                        user_id,
                        *answer,
                        unix_ms_now(),
                        premium,
                    );
                }
                true
            }
            SettingsPayload::StarCount(stars) => {
                if purpose == Some(RequestPurpose::GetPaidMessageRevenue)
                    && let Some(user_id) = pending.and_then(|p| p.user_id)
                {
                    self.payments.paid_messages.revenue.insert(user_id, *stars);
                }
                if let Some(RequestPurpose::Threads(
                    ThreadsPurpose::GetDirectMessagesChatTopicRevenue { topic_id },
                )) = purpose
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    self.set_direct_topic_revenue(chat_id, topic_id, *stars);
                }
                true
            }
            SettingsPayload::NetworkStatistics(usage) => {
                if purpose == Some(RequestPurpose::GetNetworkStatistics) {
                    self.settings.privacy_data.network.succeed(usage.clone());
                }
                true
            }
            SettingsPayload::RecoveryEmailAddress => {
                if purpose == Some(RequestPurpose::CheckRememberedPassword)
                    && self.settings.privacy_data.password_check.waits_for(pending)
                {
                    // The finish step shows until the user taps Done, which
                    // sends `hideSuggestedAction`.
                    self.settings.privacy_data.password_check = PasswordCheck::Remembered;
                }
                true
            }
            SettingsPayload::UpdateSuggestedActions { added, removed } => {
                for name in added {
                    self.chat_list.suggestions.add_action(name);
                }
                for name in removed {
                    self.chat_list.suggestions.remove_action(name);
                }
                self.chat_list.archive_mute.note_suggested(added);
                const CHECK: &str = "suggestedActionCheckPassword";
                if added.iter().any(|name| name == CHECK) {
                    self.settings.privacy_data.check_password_suggested = true;
                }
                if removed.iter().any(|name| name == CHECK) {
                    self.settings.privacy_data.check_password_suggested = false;
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
                let network = &mut self.settings.privacy_data.network;
                *network = if network.is_loading() {
                    crate::state::fetch::Fetch::Loading { stale: None }
                } else {
                    crate::state::fetch::Fetch::Idle
                };
            }
            Some(RequestPurpose::HideCheckPasswordSuggestion) => {
                self.settings.privacy_data.check_password_suggested = false;
            }
            Some(RequestPurpose::Settings(SettingsPurpose::SetInactiveSessionTtl { .. })) => {
                self.settings
                    .privacy_data
                    .clear_error(PrivacyControl::SessionTtl);
            }
            Some(RequestPurpose::Settings(SettingsPurpose::SetPrivacyRules { key })) => {
                self.settings
                    .finish_privacy_save(key, pending.map(|p| p.id));
            }
            Some(RequestPurpose::AllowUnpaidMessages) => {
                if let Some(pending) = pending {
                    self.payments.paid_messages.save.settle(pending.id);
                    if let Some(user_id) = pending.user_id {
                        self.finish_unpaid_exception(user_id);
                    }
                }
            }
            Some(RequestPurpose::SetDirectMessagesGroup) => {
                if let Some(pending) = pending {
                    self.payments.paid_messages.save.settle(pending.id);
                }
            }
            _ => {}
        }
    }

    /// Failure of this block's purposes.
    pub(crate) fn apply_privacy_data_error(
        &mut self,
        purpose: RequestPurpose,
        pending: Option<&PendingRequest>,
        err: &TdError,
    ) {
        match purpose {
            RequestPurpose::Settings(SettingsPurpose::HideSuggestedAction { action }) => {
                self.chat_list.suggestions.add_action(action);
                self.chats_state.chat_action_error = Some(format!(
                    "could not hide the suggestion (error {})",
                    err.code
                ));
                return;
            }
            RequestPurpose::HideContactCloseBirthdays => {
                self.chat_list.suggestions.birthdays_hidden = false;
                self.chats_state.chat_action_error = Some(format!(
                    "could not hide the suggestion (error {})",
                    err.code
                ));
                return;
            }
            _ => {}
        }
        let data = &mut self.settings.privacy_data;
        match purpose {
            // Only the first fetch can fail the row; a refresh that fails
            // leaves the value the row already shows.
            RequestPurpose::GetNewChatPrivacy => {
                if matches!(
                    data.new_chat,
                    Some(NewChatPrivacyState::Loading(live)) if Some(live) == pending.map(|p| p.id)
                ) {
                    data.new_chat = Some(NewChatPrivacyState::Failed);
                }
            }
            RequestPurpose::Settings(SettingsPurpose::SetNewChatPrivacy {
                previous_allow,
                previous_stars,
            }) => {
                // Roll the optimistic choice back to what the server had.
                if matches!(data.new_chat, Some(NewChatPrivacyState::Ready(_))) {
                    data.new_chat = Some(NewChatPrivacyState::Ready(NewChatPrivacy {
                        allow_from_unknown: previous_allow,
                        incoming_paid_message_star_count: previous_stars,
                    }));
                }
                data.fail(
                    PrivacyControl::NewChat,
                    sessions_error_line("change who can message you", err),
                );
            }
            RequestPurpose::SetGiftSettings => {
                data.fail(
                    PrivacyControl::Gifts,
                    sessions_error_line("change the gift settings", err),
                );
            }
            RequestPurpose::Settings(SettingsPurpose::SetInactiveSessionTtl { previous }) => {
                // Back to what the server had: `None` reads as still
                // loading, and nothing refetches it.
                data.inactive_session_ttl_days = previous;
                data.fail(
                    PrivacyControl::SessionTtl,
                    sessions_error_line("change the session timeout", err),
                );
            }
            RequestPurpose::SetSensitiveContent => {
                data.fail(
                    PrivacyControl::Sensitive,
                    sessions_error_line("change the 18+ setting", err),
                );
            }
            RequestPurpose::SetContactJoinedNotifications => {
                data.fail(
                    PrivacyControl::ContactJoined,
                    sessions_error_line("change the contact-joined notifications", err),
                );
            }
            RequestPurpose::CanSendMessageToUser => {
                // The chat stays as it was; the next open asks again.
                if let Some(user_id) = pending.and_then(|p| p.user_id) {
                    self.payments
                        .paid_messages
                        .ask_failed(user_id, unix_ms_now());
                }
            }
            RequestPurpose::GetPaidMessageRevenue | RequestPurpose::LoadDirectMessagesChat => {
                // Nothing to show: the Remove Fee box hides the refund line.
            }
            RequestPurpose::AllowUnpaidMessages | RequestPurpose::SetDirectMessagesGroup => {
                // The failure lands on the save it answers (its target is
                // the one that save was for); another request's error
                // leaves the slot alone.
                if let Some(pending) = pending {
                    self.payments.paid_messages.save.fail(
                        pending.id,
                        sessions_error_line("change the paid message settings", err),
                    );
                }
            }
            RequestPurpose::GetNetworkStatistics => {
                data.network
                    .fail(sessions_error_line("load the network usage", err));
            }
            RequestPurpose::ResetNetworkStatistics => {
                data.network
                    .fail(sessions_error_line("reset the network usage", err));
            }
            RequestPurpose::CheckRememberedPassword if data.password_check.waits_for(pending) => {
                data.password_check = if err.class == ErrorClass::Invalid {
                    PasswordCheck::Wrong
                } else {
                    PasswordCheck::Failed
                };
            }
            _ => {}
        }
    }

    /// `allowUnpaidMessagesFromUser` went through: the user writes for
    /// free now, so the Remove Fee bar and the paid total go away. The save
    /// slot is settled by the caller, against the request that answered.
    pub fn finish_unpaid_exception(&mut self, user_id: i64) {
        let paid = &mut self.payments.paid_messages;
        paid.revenue.remove(&user_id);
        if let Some(info) = self.users_state.user_full_infos.get_mut(&user_id) {
            info.extras.incoming_paid_message_star_count = 0;
        }
    }

    /// Optimistic write of the new-chat row (kept when the answer is `ok`).
    pub fn set_new_chat_privacy_local(&mut self, settings: NewChatPrivacy) {
        self.settings.privacy_data.new_chat = Some(NewChatPrivacyState::Ready(settings));
    }

    /// The own user's gift settings once `userFullInfo` was seen.
    pub fn my_gift_settings(&self) -> Option<crate::privacy::GiftSettings> {
        let me = self.my_user_id?;
        self.users_state
            .user_full_infos
            .get(&me)?
            .extras
            .gift_settings
    }

    /// Optimistic write of the own gift settings.
    pub fn set_my_gift_settings_local(&mut self, settings: crate::privacy::GiftSettings) {
        if let Some(me) = self.my_user_id
            && let Some(info) = self.users_state.user_full_infos.get_mut(&me)
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
            PrivacySettingKey::AllowCalls => self.calls.privacy_allow_calls = detail.who,
            PrivacySettingKey::PeerToPeer => self.calls.privacy_p2p = detail.who,
            _ => {}
        }
    }
}
