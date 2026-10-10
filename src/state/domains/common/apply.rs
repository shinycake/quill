//! Applies TDLib updates and answers for options, connection state and scalar answers shared by many requests.
use crate::state::*;
use crate::telegram::envelope::CommonPayload;

impl Session {
    /// Applies one common payload; called by
    /// [`Session::apply_payload`].
    pub(crate) fn apply_common_payload(
        &mut self,
        payload: CommonPayload,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        match payload {
            CommonPayload::AccountExport(value) => {
                if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::ExportAccount
                    && let Some(export) = self.account_export.as_mut()
                {
                    export.reply(pending.id, value);
                }
            }
            // MED4: `updateOption` (schema:10926). Only
            // `message_caption_length_max` is consumed (caption edits /
            // media-send captions); every other option parses but is
            // ignored, never an error.
            CommonPayload::UpdateOption { name, value } => {
                self.storage_limits.apply_option(&name, &value);
                self.folder_limits.apply_option(&name, &value);
                self.apply_privacy_option(&name, &value);
                if name == "disable_contact_registered_notifications"
                    && let OptionValue::Boolean(off) = &value
                {
                    self.disable_contact_registered_notifications = *off;
                }
                if name == "disable_top_chats"
                    && let OptionValue::Boolean(off) = &value
                {
                    self.search.top_chats_disabled = *off;
                    if *off {
                        self.search.top_chats.clear();
                        self.search.top_menu = None;
                    }
                }
                if name == "my_id"
                    && let OptionValue::Integer(id) = &value
                    && *id > 0
                {
                    self.my_user_id = Some(*id);
                }
                if name == "prefer_ipv6"
                    && let OptionValue::Boolean(on) = &value
                {
                    self.proxy.prefer_ipv6 = *on;
                }
                if name == "is_premium" {
                    self.premium_option = match &value {
                        OptionValue::Boolean(on) => Some(*on),
                        _ => Some(false),
                    };
                }
                if name == "gift_text_length_max"
                    && let OptionValue::Integer(limit) = &value
                {
                    self.gift_text_length_max = usize::try_from(*limit).ok();
                }
                if let OptionValue::Integer(limit) = &value {
                    match name.as_str() {
                        "notification_sound_size_max" => self.tone_limits.max_size = *limit,
                        "notification_sound_duration_max" => {
                            self.tone_limits.max_duration =
                                (*limit).clamp(0, i64::from(i32::MAX)) as i32
                        }
                        "notification_sound_count_max" => {
                            self.tone_limits.max_count = usize::try_from(*limit).unwrap_or(0)
                        }
                        _ => {}
                    }
                }
                if name == "pending_text_message_period"
                    && let OptionValue::Integer(period) = &value
                {
                    self.pending_bot_period_secs = u64::try_from(*period).unwrap_or(0);
                }
                if name == "animation_search_bot_username" {
                    let username = match &value {
                        OptionValue::String(name) => name.clone(),
                        _ => String::new(),
                    };
                    if self.gifs.search_bot_username != username {
                        self.gifs.search_bot_username = username;
                        self.gifs.search_bot_user_id = None;
                    }
                }
                if name == "message_caption_length_max"
                    && let OptionValue::Integer(limit) = value
                {
                    self.message_caption_length_max = limit.max(0).min(i64::from(i32::MAX)) as i32;
                }
                // R8: plain-text limit (tdesktop `messageLengthCurrent`).
                // A zero or negative value would make every send look
                // oversized, so floor it at 1.
                if name == "message_text_length_max"
                    && let OptionValue::Integer(limit) = value
                {
                    self.message_text_length_max = limit.clamp(1, i64::from(i32::MAX)) as i32;
                }
                // Slice CL1: pin-limit options (schema:13674) for the
                // client-side pin pre-check.
                if (name == "pinned_chat_count_max" || name == "pinned_archived_chat_count_max")
                    && let OptionValue::Integer(limit) = value
                {
                    let limit = limit.max(0).min(i64::from(i32::MAX)) as i32;
                    if name == "pinned_chat_count_max" {
                        self.pinned_chat_count_max = limit;
                    } else {
                        self.pinned_archived_chat_count_max = limit;
                    }
                }
            }
            CommonPayload::UpdateConnectionState(state) => self.connection = state,
            CommonPayload::UpdateDiceEmojis { emojis } => self.sync.set_dice_emojis(emojis),
            CommonPayload::UpdateFreezeState(state) => self.sync.set_freeze(state),
            CommonPayload::UpdateSpeechRecognitionTrial(trial) => self.sync.set_speech_trial(trial),
            CommonPayload::UpdateAgeVerificationParameters { parameters } => {
                self.sync.set_age_verification(parameters)
            }
            CommonPayload::Count { count } => {
                if let Some(RequestPurpose::Users(UsersPurpose::GetChatMessageCount { filter })) =
                    pending.map(|p| p.purpose)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    self.chat_media_counts
                        .entry(chat_id.0)
                        .or_default()
                        .insert(filter, count);
                }
            }
            CommonPayload::UpdateServiceNotification { kind, text } => {
                self.apply_service_notification(kind, text);
            }
            CommonPayload::Seconds { seconds } => self.apply_proxy_ping(pending, seconds),
            CommonPayload::Text { text } => match pending.map(|p| p.purpose) {
                Some(RequestPurpose::GetCountryCode) => {
                    let iso = text.trim().to_ascii_uppercase();
                    if iso.len() == 2 && iso.chars().all(|c| c.is_ascii_alphabetic()) {
                        self.guessed_country_iso = Some(iso);
                    }
                }
                Some(RequestPurpose::Calls(CallsPurpose::JoinVideoChat { group_call_id })) => {
                    self.set_group_call_join_payload(group_call_id, text);
                }
                Some(RequestPurpose::Calls(CallsPurpose::StartGroupCallScreenSharing {
                    group_call_id,
                })) => {
                    self.set_group_call_screen_share_answer(group_call_id, text);
                }
                _ => {}
            },
            // Phase C3a: `getVideoChatInviteLink` returns `httpUrl`.
            CommonPayload::HttpUrl { url } => {
                if let Some(RequestPurpose::Calls(CallsPurpose::GetVideoChatInviteLink {
                    group_call_id,
                })) = pending.map(|p| p.purpose)
                {
                    self.set_group_call_invite_link(group_call_id, url);
                // B1: `getLoginUrl` returns `httpUrl` too (schema 1.8.67,
                // line 7458) — the authorized URL after consent.
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::GetLoginUrl) {
                    self.last_login_url_info = Some(LoginUrlInfo::Open { url });
                }
            }
            // Slice msg-richtext-ai-tools: `fixedText` / `formattedText`
            // answers — captured by the driver before `apply` into
            // `Session::ai_composer_text`; nothing to reduce here.
            CommonPayload::FormattedText { text, entities }
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(RequestPurpose::Messages(
                        MessagesPurpose::TranslateJob { .. }
                    ))
                ) =>
            {
                if let Some(RequestPurpose::Messages(MessagesPurpose::TranslateJob { job })) =
                    pending.map(|p| p.purpose)
                {
                    self.finish_translation(job, Translation::Done { text, entities });
                }
            }
            CommonPayload::FixedText { .. } | CommonPayload::FormattedText { .. } => {}
        }
    }
}
