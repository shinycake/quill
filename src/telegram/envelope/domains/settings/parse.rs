//! Parses TDLib objects for settings: privacy, notifications, sessions, storage, proxy and the account.
use crate::data_settings::{AutoDownloadNetSettings, StorageChatStats};
use crate::ids::ChatId;
use crate::privacy::PrivacyRule;
use crate::telegram::envelope::*;
use serde_json::Value;

/// The settings domain's TDLib types; `Ok(None)` leaves
/// `type_name` to the other domains.
pub(crate) fn parse_settings_payload(
    type_name: &str,
    value: &Value,
) -> Result<Option<EnvelopePayload>, ParseError> {
    let payload = match type_name {
        "updateNotificationGroup" => Ok(EnvelopePayload::Settings(
            SettingsPayload::UpdateNotificationGroup {
                chat_id: ChatId(int53(value.get("chat_id"))?),
                total_count: value
                    .get("total_count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .sat_i32(),
                added_count: value
                    .get("added_notifications")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len),
                removed_count: value
                    .get("removed_notification_ids")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len),
            },
        )),
        "updateActiveNotifications" => Ok(EnvelopePayload::Settings(
            SettingsPayload::UpdateActiveNotifications {
                chat_ids: value
                    .get("groups")
                    .and_then(Value::as_array)
                    .map(|groups| {
                        groups
                            .iter()
                            .filter(|group| {
                                group
                                    .get("total_count")
                                    .and_then(Value::as_i64)
                                    .unwrap_or(0)
                                    > 0
                            })
                            .filter_map(|group| int53(group.get("chat_id")).ok().map(ChatId))
                            .collect()
                    })
                    .unwrap_or_default(),
            },
        )),
        "updateUnconfirmedSession" => Ok(parse_unconfirmed_session_update(value)),
        "resetPasswordResultOk" | "resetPasswordResultPending" | "resetPasswordResultDeclined" => {
            Ok(parse_reset_password_result(type_name, value))
        }
        // `parity:proxy-settings`: schema 1.8.67 :10118 / :10121 / :10077.
        "addedProxies" => Ok(EnvelopePayload::Settings(SettingsPayload::AddedProxies {
            proxies: crate::proxy::parse_added_proxies(value),
        })),
        "addedProxy" => Ok(EnvelopePayload::Settings(SettingsPayload::AddedProxy {
            proxy: crate::proxy::parse_added_proxy(value),
        })),
        // Phase C2i: `userPrivacySettingRules` (schema 1.8.67, :8976)
        // answers `getUserPrivacySettingRules` (:15620). Only the rule
        // constructor names are kept — enough to map Everybody /
        // Contacts / Nobody.
        "userPrivacySettingRules" => {
            let rules = value
                .get("rules")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            Ok(EnvelopePayload::Settings(
                SettingsPayload::UserPrivacySettingRules {
                    rules: rules.iter().map(PrivacyRule::parse).collect(),
                },
            ))
        }
        // Slice S3: `updateUserPrivacySettingRules` (schema 1.8.67,
        // :10871) — rules changed on another device.
        "updateUserPrivacySettingRules" => {
            let rules = value
                .get("rules")
                .and_then(|v| v.get("rules"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            Ok(EnvelopePayload::Settings(
                SettingsPayload::UpdateUserPrivacySettingRules {
                    setting: value
                        .get("setting")
                        .and_then(|v| v.get("@type"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    rules: rules.iter().map(PrivacyRule::parse).collect(),
                },
            ))
        }
        // Slice S3: `readDatePrivacySettings` (schema 1.8.67, :9026) —
        // the `getReadDatePrivacySettings` answer.
        "readDatePrivacySettings" => Ok(EnvelopePayload::Settings(
            SettingsPayload::ReadDatePrivacySettings {
                show_read_date: value
                    .get("show_read_date")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            },
        )),
        // Slice S3: `messageSenders` (schema 1.8.67, :14505) — the
        // `getBlockedMessageSenders` answer; only user senders kept.
        "messageSenders" => {
            let senders = value
                .get("senders")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            Ok(EnvelopePayload::Settings(
                SettingsPayload::BlockedMessageSenders {
                    total_count: value
                        .get("total_count")
                        .and_then(Value::as_i64)
                        .unwrap_or(0)
                        .sat_i32(),
                    sender_ids: senders
                        .iter()
                        .filter(|s| {
                            s.get("@type").and_then(Value::as_str) == Some("messageSenderUser")
                        })
                        .filter_map(|s| s.get("user_id"))
                        .filter_map(Value::as_i64)
                        .collect(),
                    senders: senders
                        .iter()
                        .filter_map(|s| parse_message_sender(Some(s)).ok())
                        .collect(),
                },
            ))
        }
        // `addSavedNotificationSound` answers with the one new sound.
        "notificationSound" => Ok(EnvelopePayload::Settings(
            SettingsPayload::NotificationSounds {
                sounds: parse_notification_sound(value).into_iter().collect(),
            },
        )),
        "notificationSounds" => {
            let sounds = value
                .get("notification_sounds")
                .and_then(Value::as_array)
                .map(|list| list.iter().filter_map(parse_notification_sound).collect())
                .unwrap_or_default();
            Ok(EnvelopePayload::Settings(
                SettingsPayload::NotificationSounds { sounds },
            ))
        }
        // Phase S2: `storageStatistics` — aggregate `by_chat[].by_file_type[]`
        // into per-`fileType` totals (TGX `TGStorageStats` aggregates the
        // same way; schema 1.8.67 lines 9780/9787/9793). Zero-size entries
        // are kept: the UI orders by a fixed category list, not by size.
        // Slice S4: per-chat rows are also kept verbatim (the usage
        // screen's per-chat breakdown; present when `chat_limit` > 0).
        "storageStatistics" => {
            let total_size = int53_or_zero(value.get("size"));
            let mut totals: Vec<StorageFileTypeStats> = Vec::new();
            let mut by_chat: Vec<StorageChatStats> = Vec::new();
            for chat in value
                .get("by_chat")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let chat_id = chat.get("chat_id").and_then(Value::as_i64).unwrap_or(0);
                let chat_size = int53_or_zero(chat.get("size"));
                let chat_count = chat
                    .get("count")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .clamp(0, i32::MAX as i64) as i32;
                by_chat.push(StorageChatStats {
                    chat_id,
                    size: chat_size,
                    count: chat_count,
                });
                for entry in chat
                    .get("by_file_type")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let Some(file_type) = entry
                        .get("file_type")
                        .and_then(|t| t.get("@type"))
                        .and_then(Value::as_str)
                    else {
                        continue;
                    };
                    let size = int53_or_zero(entry.get("size"));
                    let count = entry
                        .get("count")
                        .and_then(Value::as_i64)
                        .unwrap_or(0)
                        .clamp(0, i32::MAX as i64) as i32;
                    match totals.iter_mut().find(|t| t.file_type == file_type) {
                        Some(existing) => {
                            existing.size = existing.size.saturating_add(size);
                            existing.count = existing.count.saturating_add(count);
                        }
                        None => totals.push(StorageFileTypeStats {
                            file_type: file_type.to_string(),
                            size,
                            count,
                        }),
                    }
                }
            }
            Ok(EnvelopePayload::Settings(
                SettingsPayload::StorageStatistics {
                    total_size,
                    by_file_type: totals,
                    by_chat,
                },
            ))
        }
        // Slice S4: `autoDownloadSettingsPresets` — the
        // `getAutoDownloadSettingsPresets` response (schema 1.8.67, line
        // 9862). Missing low/medium/high is a malformed answer, not a
        // default — the reducer never sees it.
        "autoDownloadSettingsPresets" => {
            let Some((low, medium, high)) = AutoDownloadNetSettings::parse_presets(value) else {
                return Err(ParseError::MissingField);
            };
            Ok(EnvelopePayload::Settings(
                SettingsPayload::AutoDownloadSettingsPresets { low, medium, high },
            ))
        }
        // Slice A2: `passwordState` — the `getPasswordState` /
        // `setPassword` / `setRecoveryEmailAddress` /
        // `resendRecoveryEmailAddressCode` /
        // `cancelRecoveryEmailAddressVerification` response (schema
        // 1.8.67, line 273). `recovery_email_address_code_info` is null
        // unless a recovery-email confirmation is pending (schema line
        // 83); a non-object there is treated as absent, never an error.
        "passwordState" => {
            let bool_field = |name: &str| value.get(name).and_then(Value::as_bool).unwrap_or(false);
            let str_field = |name: &str| {
                value
                    .get(name)
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string()
            };
            let (pending_email_pattern, pending_email_code_length) = value
                .get("recovery_email_address_code_info")
                .and_then(Value::as_object)
                .map(|info| {
                    (
                        info.get("email_address_pattern")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        info.get("length")
                            .and_then(Value::as_i64)
                            .unwrap_or(0)
                            .clamp(0, i32::MAX as i64) as i32,
                    )
                })
                .unwrap_or((None, 0));
            Ok(EnvelopePayload::Settings(SettingsPayload::PasswordState {
                state: PasswordState {
                    has_password: bool_field("has_password"),
                    password_hint: str_field("password_hint"),
                    has_recovery_email_address: bool_field("has_recovery_email_address"),
                    has_passport_data: bool_field("has_passport_data"),
                    pending_email_pattern,
                    pending_email_code_length,
                    login_email_address_pattern: str_field("login_email_address_pattern"),
                    pending_reset_date: value
                        .get("pending_reset_date")
                        .and_then(Value::as_i64)
                        .unwrap_or(0)
                        .clamp(0, i32::MAX as i64) as i32,
                },
            }))
        }
        "accountTtl" => {
            let days = value
                .get("days")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .clamp(0, i32::MAX as i64) as i32;
            Ok(EnvelopePayload::Settings(SettingsPayload::AccountTtl {
                days,
            }))
        }
        // Slice A3: `sessions` — the `getActiveSessions` answer (schema
        // 1.8.67, lines 9144/9147). Malformed entries are dropped rather
        // than failing the whole list (a session id is required).
        "session" => Ok(EnvelopePayload::Settings(
            SettingsPayload::DeviceLoginResult {
                result: parse_session(value)
                    .map(|session| {
                        if session.is_password_pending {
                            crate::auth::DeviceLoginResult::PasswordRequired
                        } else {
                            crate::auth::DeviceLoginResult::Linked
                        }
                    })
                    .unwrap_or(crate::auth::DeviceLoginResult::Failed),
            },
        )),
        "sessions" => {
            let sessions = value
                .get("sessions")
                .and_then(Value::as_array)
                .map(|list| list.iter().filter_map(parse_session).collect())
                .unwrap_or_default();
            let inactive_session_ttl_days = value
                .get("inactive_session_ttl_days")
                .and_then(Value::as_i64)
                .and_then(|days| i32::try_from(days).ok())
                .filter(|days| *days > 0);
            Ok(EnvelopePayload::Settings(SettingsPayload::Sessions {
                sessions,
                inactive_session_ttl_days,
            }))
        }
        // B13: privacy and data settings answers.
        "newChatPrivacySettings" => Ok(EnvelopePayload::Settings(
            SettingsPayload::NewChatPrivacySettings(crate::privacy::NewChatPrivacy::from_value(
                value,
            )),
        )),
        "networkStatistics" => Ok(EnvelopePayload::Settings(
            SettingsPayload::NetworkStatistics(crate::network_usage::NetworkUsage::from_value(
                value,
            )),
        )),
        "recoveryEmailAddress" => Ok(EnvelopePayload::Settings(
            SettingsPayload::RecoveryEmailAddress,
        )),
        "updateSuggestedActions" => {
            let names = |key: &str| -> Vec<String> {
                value
                    .get(key)
                    .and_then(Value::as_array)
                    .map(|list| {
                        list.iter()
                            .filter_map(|a| a.get("@type").and_then(Value::as_str))
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default()
            };
            Ok(EnvelopePayload::Settings(
                SettingsPayload::UpdateSuggestedActions {
                    added: names("added_actions"),
                    removed: names("removed_actions"),
                },
            ))
        }
        "updateContactCloseBirthdays" => {
            let users = value
                .get("close_birthday_users")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(|entry| {
                            let user_id = entry.get("user_id").and_then(Value::as_i64)?;
                            let date = entry.get("birthdate")?;
                            Some(crate::chatlist_suggestions::CloseBirthday {
                                user_id,
                                day: date.get("day").and_then(Value::as_u64)? as u8,
                                month: date.get("month").and_then(Value::as_u64)? as u8,
                                year: date
                                    .get("year")
                                    .and_then(Value::as_i64)
                                    .filter(|year| *year > 0)
                                    .map(|year| year as i32),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok(EnvelopePayload::Settings(
                SettingsPayload::UpdateContactCloseBirthdays { users },
            ))
        }
        // Slice A4: `connectedWebsites` — the `getConnectedWebsites`
        // answer (schema 1.8.67, lines 9171/15124). Unparseable websites
        // are skipped rather than failing the whole list (a website id
        // is required).
        "connectedWebsites" => {
            let websites = value
                .get("websites")
                .and_then(Value::as_array)
                .map(|list| list.iter().filter_map(parse_website).collect())
                .unwrap_or_default();
            Ok(EnvelopePayload::Settings(
                SettingsPayload::ConnectedWebsites { websites },
            ))
        }
        "updateSavedNotificationSounds" => Ok(EnvelopePayload::Settings(
            SettingsPayload::UpdateSavedNotificationSounds {
                sound_ids: value
                    .get("notification_sound_ids")
                    .and_then(Value::as_array)
                    .map(|ids| {
                        ids.iter()
                            .filter_map(|id| id.as_i64().or_else(|| id.as_str()?.parse().ok()))
                            .collect()
                    })
                    .unwrap_or_default(),
            },
        )),
        "scopeNotificationSettings" => {
            // The response to `getScopeNotificationSettings` carries no scope
            // field — the scope is correlated via the pending request.
            Ok(EnvelopePayload::Settings(
                SettingsPayload::ScopeNotificationSettings {
                    scope: NotificationSettingsScope::PrivateChats,
                    settings: parse_scope_notification_settings(Some(value)),
                },
            ))
        }
        "updateScopeNotificationSettings" => {
            let scope = value
                .get("scope")
                .and_then(|s| s.get("@type"))
                .and_then(Value::as_str);
            match parse_notification_settings_scope(scope) {
                Some(scope) => Ok(EnvelopePayload::Settings(
                    SettingsPayload::UpdateScopeNotificationSettings {
                        scope,
                        settings: parse_scope_notification_settings(
                            value.get("notification_settings"),
                        ),
                    },
                )),
                None => Err(ParseError::MissingField),
            }
        }
        "updateReactionNotificationSettings" => Ok(EnvelopePayload::Settings(
            SettingsPayload::UpdateReactionNotificationSettings {
                settings: parse_reaction_notification_settings(value.get("notification_settings")),
            },
        )),
        "languagePackInfo" => Ok(EnvelopePayload::Settings(
            SettingsPayload::LanguagePackInfo(LanguagePackInfoData::parse(value)),
        )),
        _ => return Ok(None),
    };
    payload.map(Some)
}
