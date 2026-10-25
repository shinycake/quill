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
                    && let Some(export) = self.settings.account_export.as_mut()
                {
                    export.link.reply(pending.id, value);
                } else if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::ExportChat
                    && let Some(export) = self.settings.chat_export.as_mut()
                {
                    export.link.reply(pending.id, value);
                } else if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::ExportRichHtml
                    && let Some(export) = self
                        .settings
                        .rich_html_exports
                        .iter_mut()
                        .find(|export| export.link.pending == Some(pending.id))
                {
                    export.link.reply(pending.id, value);
                }
            }
            // MED4: `updateOption` (schema:10926). Only
            // `message_caption_length_max` is consumed (caption edits /
            // media-send captions); every other option parses but is
            // ignored, never an error.
            CommonPayload::UpdateOption { name, value } => {
                self.settings.storage_limits.apply_option(&name, &value);
                self.chat_list.folder_limits.apply_option(&name, &value);
                self.chat_list.archive_mute.apply_option(&name, &value);
                self.apply_privacy_option(&name, &value);
                self.users_state.apply_option(&name, &value);
                self.apply_quick_replies_option(&name, &value);
                self.apply_business_option(&name, &value);
                self.apply_earn_option(&name, &value);
                self.apply_ai_compose_option(&name, &value);
                self.stories.stealth_periods.apply_option(&name, &value);
                if name == "disable_contact_registered_notifications"
                    && let OptionValue::Boolean(off) = &value
                {
                    self.settings.disable_contact_registered_notifications = *off;
                }
                if name == "disable_top_chats"
                    && let OptionValue::Boolean(off) = &value
                {
                    self.search.search.top_chats_disabled = *off;
                    if *off {
                        self.search.search.top_chats.clear();
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
                    self.settings.proxy.prefer_ipv6 = *on;
                }
                if name == "is_premium" {
                    self.payments.premium_option = match &value {
                        OptionValue::Boolean(on) => Some(*on),
                        _ => Some(false),
                    };
                }
                if name == "is_premium_available"
                    && let OptionValue::Boolean(on) = &value
                {
                    self.payments.premium_available = Some(*on);
                }
                if name == crate::ton_wallet::CAN_USE_OPTION {
                    self.ton_wallet
                        .set_can_use(matches!(value, OptionValue::Boolean(true)));
                }
                if name == "paid_reaction_star_count_max"
                    && let OptionValue::Integer(limit) = &value
                    && *limit > 0
                {
                    self.payments.paid.max_stars = Some(*limit);
                }
                if name == "paid_group_call_message_star_count_max"
                    && let OptionValue::Integer(limit) = &value
                    && *limit > 0
                {
                    self.stories.live_max_stars = Some(*limit);
                }
                if name == "poll_country_count_max"
                    && let OptionValue::Integer(limit) = &value
                    && *limit > 0
                {
                    self.payments.poll_countries_max = usize::try_from(*limit).ok();
                }
                if name == "poll_open_period_max"
                    && let OptionValue::Integer(limit) = &value
                    && *limit > 0
                {
                    self.payments.poll_open_period_max = Some(*limit);
                }
                if name == "gift_text_length_max"
                    && let OptionValue::Integer(limit) = &value
                {
                    self.payments.gift_text_length_max = usize::try_from(*limit).ok();
                }
                if let OptionValue::Integer(limit) = &value {
                    self.payments.giveaway.limits.apply_option(&name, *limit);
                    self.payments
                        .suggested_posts
                        .limits
                        .apply_option(&name, *limit);
                    match name.as_str() {
                        "notification_sound_size_max" => {
                            self.messages.tone_limits.max_size = *limit
                        }
                        "notification_sound_duration_max" => {
                            self.messages.tone_limits.max_duration =
                                (*limit).clamp(0, i64::from(i32::MAX)) as i32
                        }
                        "subscription_star_count_max" if *limit > 0 => {
                            self.groups.subscription_star_max = Some(*limit)
                        }
                        "supergroup_size_max" => {
                            self.groups.supergroup_size_max =
                                i32::try_from(*limit).unwrap_or(i32::MAX)
                        }
                        "notification_sound_count_max" => {
                            self.messages.tone_limits.max_count =
                                usize::try_from(*limit).unwrap_or(0)
                        }
                        _ => {}
                    }
                }
                if name == "pending_text_message_period"
                    && let OptionValue::Integer(period) = &value
                {
                    self.messages.pending_bot_period_secs = u64::try_from(*period).unwrap_or(0);
                }
                if name == "animation_search_bot_username" {
                    let username = match &value {
                        OptionValue::String(name) => name.clone(),
                        _ => String::new(),
                    };
                    if self.stickers.gifs.search_bot_username != username {
                        self.stickers.gifs.search_bot_username = username;
                        self.stickers.gifs.search_bot_user_id = None;
                    }
                }
                if name == "venue_search_bot_username" {
                    self.location.set_venue_bot_username(match &value {
                        OptionValue::String(name) => name.clone(),
                        _ => String::new(),
                    });
                }
                if name == "message_caption_length_max"
                    && let OptionValue::Integer(limit) = value
                {
                    self.messages.message_caption_length_max =
                        limit.max(0).min(i64::from(i32::MAX)) as i32;
                }
                // The dearest price of paid media (schema `inputMessagePaidMedia`).
                if name == "paid_media_message_star_count_max"
                    && let OptionValue::Integer(limit) = value
                    && limit > 0
                {
                    self.messages.paid_media_star_max = limit;
                }
                // R8: plain-text limit (tdesktop `messageLengthCurrent`).
                // A zero or negative value would make every send look
                // oversized, so floor it at 1.
                if name == "message_text_length_max"
                    && let OptionValue::Integer(limit) = value
                {
                    self.messages.message_text_length_max =
                        limit.clamp(1, i64::from(i32::MAX)) as i32;
                }
                // Slice CL1: pin-limit options (schema:13674) for the
                // client-side pin pre-check.
                if (name == "pinned_chat_count_max" || name == "pinned_archived_chat_count_max")
                    && let OptionValue::Integer(limit) = value
                {
                    let limit = limit.max(0).min(i64::from(i32::MAX)) as i32;
                    if name == "pinned_chat_count_max" {
                        self.chat_list.pinned_chat_count_max = limit;
                    } else {
                        self.chat_list.pinned_archived_chat_count_max = limit;
                    }
                }
            }
            CommonPayload::UpdateConnectionState(state) => {
                // Back online: stories that failed while offline are asked
                // for again at once.
                if state == ConnectionState::Ready && self.connection != ConnectionState::Ready {
                    self.stories.story_retry.clear();
                }
                self.connection = state;
            }
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
                    self.media
                        .chat_media_counts
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
                        self.auth_state.guessed_country_iso = Some(iso);
                    }
                }
                Some(RequestPurpose::Settings(SettingsPurpose::Passkeys(
                    crate::passkeys::PasskeysPurpose::Parameters,
                ))) => self.apply_passkey_parameters(&text),
                Some(RequestPurpose::Settings(SettingsPurpose::Passkeys(
                    crate::passkeys::PasskeysPurpose::LoginParameters,
                ))) => self.apply_passkey_login_parameters(&text),
                Some(RequestPurpose::Bots(purpose @ BotsPurpose::GetBotEditText { .. })) => {
                    self.apply_owned_bot_text(purpose, text);
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
                if self.is_earn_request(pending) {
                    // Revenue: a Fragment withdrawal page or the ads account.
                    self.apply_earn_url(url, pending);
                } else if pending.map(|p| p.purpose)
                    == Some(RequestPurpose::Payments(PaymentsPurpose::GiftExportUrl))
                {
                    // "Send via Blockchain": Fragment's export page.
                    self.apply_gift_export_url(url);
                } else if pending.map(|p| p.purpose)
                    == Some(RequestPurpose::Payments(PaymentsPurpose::Grams(
                        crate::premium_hub::ton::GramsPurpose::WithdrawalUrl,
                    )))
                {
                    // Collect: Fragment's withdrawal page.
                    self.apply_gram_withdrawal_url(url, pending.map(|p| p.id));
                } else if pending.map(|p| p.purpose)
                    == Some(RequestPurpose::Payments(PaymentsPurpose::GiftExportProbe))
                {
                    // The password-less probe: the URL is dropped unread.
                    drop(url);
                    self.apply_gift_export_probe_answer();
                } else if let Some(RequestPurpose::Calls(CallsPurpose::GetVideoChatInviteLink {
                    group_call_id,
                    can_self_unmute,
                })) = pending.map(|p| p.purpose)
                {
                    self.set_group_call_invite_link(group_call_id, can_self_unmute, url);
                // B1: `getLoginUrl` returns `httpUrl` too (schema 1.8.67,
                // line 7458) — the authorized URL after consent.
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::GetLoginUrl) {
                    self.bots.last_login_url_info = Some(LoginUrlInfo::Open { url });
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::GetBackgroundUrl) {
                    self.chats_state.background_url = Some(url);
                }
            }
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
            CommonPayload::FormattedText { text, entities } => {
                self.apply_ai_compose_formatted_text(pending, text, entities);
            }
        }
    }
}
