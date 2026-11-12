//! B13: reducer state for the privacy / security / data settings that sit
//! next to the rule screen: new-chat privacy ("Who can message me"), the
//! inactive-session TTL, the 18+ content option, network usage, and the
//! "Do you still remember your password?" check.
use super::*;
use crate::network_usage::NetworkUsage;
use crate::privacy::{GiftSave, NewChatPrivacy, NewChatPrivacyState};
use crate::state::saves::Optimistic;
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
    /// `getNewChatPrivacySettings` state; `None` = never asked. Saves in
    /// flight live inside the stage.
    pub new_chat: Option<NewChatPrivacyState>,
    /// The refresh of a value already shown, in flight as this request (a
    /// second open sends nothing; its answer or error frees the slot).
    pub new_chat_refresh: Option<RequestId>,
    /// Days of inactivity before sessions end (`getActiveSessions`), with
    /// the `setInactiveSessionTtl` saves in flight.
    pub inactive_session_ttl: Optimistic<i32>,
    /// `ignore_sensitive_content_restrictions` as TDLib last reported it.
    pub ignore_sensitive: Option<bool>,
    /// `can_ignore_sensitive_content_restrictions`: the server lets this
    /// account turn the 18+ toggle on.
    pub can_ignore_sensitive: bool,
    /// `getNetworkStatistics`: the answer, a refetch under it, or why it
    /// failed (a failed reset lands here too).
    pub network: crate::state::fetch::Fetch<NetworkUsage>,
    pub password_check: PasswordCheck,
    /// The own gift settings' saves in flight, over the confirmed value.
    pub gift_save: GiftSave,
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
    /// The server asked the client to check the password
    /// (`suggestedActionCheckPassword`): the chat-list suggestion is the one
    /// copy of that fact.
    pub fn check_password_suggested(&self) -> bool {
        self.chat_list
            .suggestions
            .actions
            .contains(crate::chatlist_suggestions::ACTION_PASSWORD)
    }

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
                    && let Some(state) = self.settings.privacy_data.new_chat.as_mut()
                {
                    state.fetched(id, *settings);
                    if self.settings.privacy_data.new_chat_refresh == Some(id) {
                        self.settings.privacy_data.new_chat_refresh = None;
                    }
                }
                true
            }
            SettingsPayload::CanSendMessageToUser(answer) => {
                if purpose == Some(RequestPurpose::CanSendMessageToUser)
                    && let Some(pending) = pending
                    && let Some(user_id) = pending.user_id()
                {
                    self.payments.paid_messages.accept_answer(
                        user_id,
                        pending.id,
                        *answer,
                        unix_ms_now(),
                    );
                }
                true
            }
            SettingsPayload::StarCount(stars) => {
                if purpose == Some(RequestPurpose::GetPaidMessageRevenue)
                    && let Some(user_id) = pending.and_then(|p| p.user_id())
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
                    self.settings
                        .privacy_data
                        .network
                        .answered(pending.map(|p| p.id), usage.clone());
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
                true
            }
            _ => false,
        }
    }

    /// `ok` of this block's purposes.
    pub(crate) fn apply_privacy_data_ok(&mut self, pending: Option<&PendingRequest>) {
        match pending.map(|p| p.purpose) {
            // The reset sender already dropped the old numbers and asked
            // again behind it; the refetch holds its own request.
            Some(RequestPurpose::ResetNetworkStatistics) => {}
            Some(RequestPurpose::HideCheckPasswordSuggestion) => {
                self.chat_list
                    .suggestions
                    .remove_action(crate::chatlist_suggestions::ACTION_PASSWORD);
            }
            Some(RequestPurpose::Settings(SettingsPurpose::SetInactiveSessionTtl)) => {
                let data = &mut self.settings.privacy_data;
                if let Some(id) = pending.map(|p| p.id) {
                    data.inactive_session_ttl.landed(id);
                }
                data.clear_error(PrivacyControl::SessionTtl);
            }
            Some(RequestPurpose::SetGiftSettings) => {
                if let Some(id) = pending.map(|p| p.id) {
                    self.settings.privacy_data.gift_save.landed(id);
                }
            }
            Some(RequestPurpose::Settings(SettingsPurpose::SetNewChatPrivacy)) => {
                if let Some(id) = pending.map(|p| p.id)
                    && let Some(state) = self.settings.privacy_data.new_chat.as_mut()
                {
                    state.save_landed(id);
                }
            }
            Some(RequestPurpose::Settings(SettingsPurpose::SetPrivacyRules { key })) => {
                self.settings
                    .finish_privacy_save(key, pending.map(|p| p.id));
            }
            Some(RequestPurpose::AllowUnpaidMessages) => {
                if let Some(pending) = pending {
                    self.payments.paid_messages.save.settle(pending.id);
                    if let Some(user_id) = pending.user_id() {
                        self.finish_unpaid_exception(user_id);
                    }
                }
            }
            Some(RequestPurpose::SetDirectMessagesGroup) => {
                if let Some(pending) = pending {
                    self.payments.paid_messages.save.settle(pending.id);
                }
            }
            Some(RequestPurpose::HideContactCloseBirthdays) => {
                if let Some(pending) = pending {
                    self.chat_list.suggestions.birthdays_hidden_by(pending.id);
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
                self.chats_state.chat_action_error.push(format!(
                    "could not hide the suggestion (error {})",
                    err.code
                ));
                return;
            }
            RequestPurpose::HideCheckPasswordSuggestion => {
                self.chat_list
                    .suggestions
                    .add_action(crate::chatlist_suggestions::ACTION_PASSWORD);
                self.chats_state.chat_action_error.push(format!(
                    "could not hide the suggestion (error {})",
                    err.code
                ));
                return;
            }
            RequestPurpose::HideContactCloseBirthdays => {
                if pending.is_some_and(|p| self.chat_list.suggestions.birthdays_hide_failed(p.id)) {
                    self.chats_state.chat_action_error.push(format!(
                        "could not hide the suggestion (error {})",
                        err.code
                    ));
                }
                return;
            }
            _ => {}
        }
        let data = &mut self.settings.privacy_data;
        match purpose {
            // Only the first fetch can fail the row; a refresh that fails
            // leaves the value the row already shows.
            RequestPurpose::GetNewChatPrivacy => {
                if data.new_chat_refresh.is_some() && data.new_chat_refresh == pending.map(|p| p.id)
                {
                    data.new_chat_refresh = None;
                }
                if matches!(
                    data.new_chat,
                    Some(NewChatPrivacyState::Loading(live)) if Some(live) == pending.map(|p| p.id)
                ) {
                    data.new_chat = Some(NewChatPrivacyState::Failed);
                }
            }
            RequestPurpose::Settings(SettingsPurpose::SetNewChatPrivacy) => {
                // The row falls back to the next save still out, or to what
                // the server had, whichever order the saves fail in.
                if let Some(id) = pending.map(|p| p.id)
                    && let Some(state) = data.new_chat.as_mut()
                {
                    state.save_failed(id);
                }
                data.fail(
                    PrivacyControl::NewChat,
                    sessions_error_line("change who can message you", err),
                );
            }
            RequestPurpose::SetGiftSettings => {
                let shown = pending.and_then(|p| data.gift_save.failed(p.id));
                if let Some(shown) = shown {
                    self.set_my_gift_settings_local(shown);
                }
                let data = &mut self.settings.privacy_data;
                data.fail(
                    PrivacyControl::Gifts,
                    sessions_error_line("change the gift settings", err),
                );
            }
            RequestPurpose::Settings(SettingsPurpose::SetInactiveSessionTtl) => {
                if let Some(id) = pending.map(|p| p.id) {
                    data.inactive_session_ttl.failed(id);
                }
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
                if let Some(pending) = pending
                    && let Some(user_id) = pending.user_id()
                {
                    self.payments
                        .paid_messages
                        .ask_failed(user_id, pending.id, unix_ms_now());
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
                data.network.failed(
                    pending.map(|p| p.id),
                    sessions_error_line("load the network usage", err),
                    false,
                );
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
