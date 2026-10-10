//! Slice S3 (privacy screen): `UserPrivacySetting` request builders —
//! `getUserPrivacySettingRules` / `setUserPrivacySettingRules` for the five
//! screen rules, `getReadDatePrivacySettings` /
//! `setReadDatePrivacySettings` for the read-date toggle, and
//! `getBlockedMessageSenders` for the blocked-users list. Blocking itself
//! reuses the CL3 `setMessageSenderBlockList` (passing a null block list
//! unblocks, TGX `Tdlib.unblockSender`).

use crate::ids::RequestId;
use crate::telegram::requests::PrivacyWho;
use serde_json::{Value, json};

/// Slice S3 (privacy screen) + B13: the `UserPrivacySetting` constructors
/// the Privacy screen edits (schema 1.8.67, :8981-:9021). Call settings
/// keep the pre-existing `CallPrivacySetting` / `call_privacy_*` plumbing
/// (Phase C2i) — the screen reads those fields directly instead of
/// re-plumbing them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrivacySettingKey {
    ShowStatus,
    ShowPhoneNumber,
    ShowProfilePhoto,
    ShowLinkInForwardedMessages,
    AllowChatInvites,
    /// B13: `userPrivacySettingShowBio` (tdesktop "Bio").
    ShowBio,
    /// B13: `userPrivacySettingShowBirthdate` (tdesktop "Date of birth").
    ShowBirthdate,
    /// B13: `userPrivacySettingShowProfileAudio` (tdesktop "Saved Music").
    ShowProfileAudio,
    /// B13: `userPrivacySettingAllowFindingByPhoneNumber` (tdesktop's
    /// "Who can find me by my number" under Phone Number).
    AllowFindingByPhoneNumber,
    /// B13: `userPrivacySettingAllowPrivateVoiceAndVideoNoteMessages`
    /// (tdesktop "Voice Messages").
    AllowVoiceMessages,
    /// B13: `userPrivacySettingAutosaveGifts` (tdesktop "Gifts": who can
    /// display gifts on the profile).
    AutosaveGifts,
    /// `userPrivacySettingAllowCalls` (tdesktop "Calls": who can call me).
    AllowCalls,
    /// `userPrivacySettingAllowPeerToPeerCalls` (tdesktop "Peer-to-peer in
    /// calls").
    PeerToPeer,
}

impl PrivacySettingKey {
    /// Verbatim `UserPrivacySetting` constructor names (schema 1.8.67,
    /// :8982-:9021).
    pub fn td_type(self) -> &'static str {
        match self {
            PrivacySettingKey::ShowStatus => "userPrivacySettingShowStatus",
            PrivacySettingKey::ShowPhoneNumber => "userPrivacySettingShowPhoneNumber",
            PrivacySettingKey::ShowProfilePhoto => "userPrivacySettingShowProfilePhoto",
            PrivacySettingKey::ShowLinkInForwardedMessages => {
                "userPrivacySettingShowLinkInForwardedMessages"
            }
            PrivacySettingKey::AllowChatInvites => "userPrivacySettingAllowChatInvites",
            PrivacySettingKey::ShowBio => "userPrivacySettingShowBio",
            PrivacySettingKey::ShowBirthdate => "userPrivacySettingShowBirthdate",
            PrivacySettingKey::ShowProfileAudio => "userPrivacySettingShowProfileAudio",
            PrivacySettingKey::AllowFindingByPhoneNumber => {
                "userPrivacySettingAllowFindingByPhoneNumber"
            }
            PrivacySettingKey::AllowVoiceMessages => {
                "userPrivacySettingAllowPrivateVoiceAndVideoNoteMessages"
            }
            PrivacySettingKey::AutosaveGifts => "userPrivacySettingAutosaveGifts",
            PrivacySettingKey::AllowCalls => "userPrivacySettingAllowCalls",
            PrivacySettingKey::PeerToPeer => "userPrivacySettingAllowPeerToPeerCalls",
        }
    }

    /// Screen row labels (tdesktop `lng_settings_*` privacy rows).
    pub fn label(self) -> &'static str {
        match self {
            PrivacySettingKey::ShowStatus => "Last Seen & Online",
            PrivacySettingKey::ShowPhoneNumber => "Phone Number",
            PrivacySettingKey::ShowProfilePhoto => "Profile Photos",
            PrivacySettingKey::ShowLinkInForwardedMessages => "Forwarded Messages",
            PrivacySettingKey::AllowChatInvites => "Groups & Channels",
            PrivacySettingKey::ShowBio => "Bio",
            PrivacySettingKey::ShowBirthdate => "Date of Birth",
            PrivacySettingKey::ShowProfileAudio => "Saved Music",
            PrivacySettingKey::AllowFindingByPhoneNumber => "Who can find me by my number",
            PrivacySettingKey::AllowVoiceMessages => "Voice Messages",
            PrivacySettingKey::AutosaveGifts => "Gifts",
            PrivacySettingKey::AllowCalls => "Who can call me",
            PrivacySettingKey::PeerToPeer => "Peer-to-peer calls",
        }
    }

    /// Every rule key the screen fetches and keeps in sync. The two call
    /// settings also feed the older `call_privacy_*` fields (see
    /// `Session::mirror_call_privacy`).
    pub const fn all() -> [PrivacySettingKey; 13] {
        [
            PrivacySettingKey::ShowStatus,
            PrivacySettingKey::ShowPhoneNumber,
            PrivacySettingKey::ShowProfilePhoto,
            PrivacySettingKey::ShowLinkInForwardedMessages,
            PrivacySettingKey::AllowChatInvites,
            PrivacySettingKey::ShowBio,
            PrivacySettingKey::ShowBirthdate,
            PrivacySettingKey::ShowProfileAudio,
            PrivacySettingKey::AllowFindingByPhoneNumber,
            PrivacySettingKey::AllowVoiceMessages,
            PrivacySettingKey::AutosaveGifts,
            PrivacySettingKey::AllowCalls,
            PrivacySettingKey::PeerToPeer,
        ]
    }

    /// Rows of the "Who can see my..." block, in tdesktop's order.
    pub const fn visibility_rows() -> [PrivacySettingKey; 8] {
        [
            PrivacySettingKey::ShowPhoneNumber,
            PrivacySettingKey::ShowStatus,
            PrivacySettingKey::ShowProfilePhoto,
            PrivacySettingKey::ShowBio,
            PrivacySettingKey::ShowBirthdate,
            PrivacySettingKey::AutosaveGifts,
            PrivacySettingKey::ShowProfileAudio,
            PrivacySettingKey::ShowLinkInForwardedMessages,
        ]
    }

    /// Rows of the "Who can contact me" block (the call rows and the
    /// new-chat row sit between them in the UI).
    pub const fn contact_rows() -> [PrivacySettingKey; 4] {
        [
            PrivacySettingKey::AllowCalls,
            PrivacySettingKey::PeerToPeer,
            PrivacySettingKey::AllowVoiceMessages,
            PrivacySettingKey::AllowChatInvites,
        ]
    }

    /// The editor's radio header (tdesktop `lng_edit_privacy_*_header`).
    pub fn header(self) -> &'static str {
        match self {
            PrivacySettingKey::ShowStatus => "Who can see my last seen time",
            PrivacySettingKey::ShowPhoneNumber => "Who can see my phone number",
            PrivacySettingKey::ShowProfilePhoto => "Who can see my profile photos",
            PrivacySettingKey::ShowLinkInForwardedMessages => {
                "Who can add a link to my account when forwarding my messages"
            }
            PrivacySettingKey::AllowChatInvites => "Who can add me to groups and channels",
            PrivacySettingKey::ShowBio => "Who can see my bio",
            PrivacySettingKey::ShowBirthdate => "Who can see my date of birth",
            PrivacySettingKey::ShowProfileAudio => "Who can see my saved music in profile",
            PrivacySettingKey::AllowFindingByPhoneNumber => "Who can find me by my number",
            PrivacySettingKey::AllowVoiceMessages => "Who can send me voice messages",
            PrivacySettingKey::AutosaveGifts => "Who can display gifts on my profile",
            PrivacySettingKey::AllowCalls => "Who can call me",
            PrivacySettingKey::PeerToPeer => "Use peer-to-peer with",
        }
    }

    /// The note under the exception rows (tdesktop
    /// `lng_edit_privacy_*_exceptions`).
    pub fn exceptions_note(self) -> &'static str {
        match self {
            PrivacySettingKey::ShowPhoneNumber => {
                "Add users or groups to override the settings above."
            }
            PrivacySettingKey::ShowBio => {
                "These users will or will not be able to see your profile bio regardless of the settings above."
            }
            PrivacySettingKey::ShowBirthdate => {
                "These users will or will not be able to see your date of birth regardless of the settings above."
            }
            PrivacySettingKey::AutosaveGifts => {
                "Choose whether gifts from specific senders need your approval before they're visible to others on your profile."
            }
            PrivacySettingKey::ShowProfileAudio => {
                "These users will or will not be able to see your saved music regardless of the settings above."
            }
            PrivacySettingKey::AllowVoiceMessages => {
                "These users will or will not be able to send voice and video messages to you regardless of the settings above."
            }
            PrivacySettingKey::AllowCalls => {
                "These users will or will not be able to call you regardless of the settings above."
            }
            PrivacySettingKey::PeerToPeer => {
                "Peer-to-peer in calls will or will not be used with these users regardless of the settings above."
            }
            _ => "Add users or groups to override the settings above.",
        }
    }

    /// Radio choices the key supports. tdesktop's "find me by number"
    /// offers only Everybody / My contacts.
    pub fn options(self) -> &'static [PrivacyWho] {
        match self {
            PrivacySettingKey::AllowFindingByPhoneNumber => {
                &[PrivacyWho::Everybody, PrivacyWho::Contacts]
            }
            _ => &[
                PrivacyWho::Everybody,
                PrivacyWho::Contacts,
                PrivacyWho::Nobody,
            ],
        }
    }

    /// Whether the key takes exception lists at all (the find-by-number
    /// rule is a plain two-way choice).
    pub fn has_exceptions(self) -> bool {
        self != PrivacySettingKey::AllowFindingByPhoneNumber
    }

    /// tdesktop `allowPremiumsToggle`: only the Always list of
    /// "Groups & Channels" offers the Premium users row.
    pub fn allows_premium_exception(self, always: bool) -> bool {
        always && self == PrivacySettingKey::AllowChatInvites
    }

    /// tdesktop `allowMiniAppsToggle`: both lists of "Gifts" offer the
    /// Mini Apps (bots) row.
    pub fn allows_bots_exception(self) -> bool {
        self == PrivacySettingKey::AutosaveGifts
    }

    /// tdesktop `VoicesPrivacyController::premiumClickedCallback`:
    /// restricting who can send voice messages needs Premium.
    pub fn restriction_needs_premium(self) -> bool {
        self == PrivacySettingKey::AllowVoiceMessages
    }
}

/// Slice S3: `getUserPrivacySettingRules` (schema 1.8.67, :15620 —
/// "Returns the current privacy settings") for any rule setting. The
/// Phase C2i `get_user_privacy_setting_rules` keeps serving the calls UI.
pub fn get_privacy_rules(extra: RequestId, setting_type: &str) -> String {
    json!({
        "@type": "getUserPrivacySettingRules",
        "@extra": extra.as_extra(),
        "setting": {"@type": setting_type},
    })
    .to_string()
}

/// Slice S3: `setUserPrivacySettingRules` (schema 1.8.67, :15617 —
/// "Changes user privacy settings") for any rule setting. `rules` is
/// the full `userPrivacySettingRules` list (see
/// `PrivacyRuleDetail::recompose`).
pub fn set_privacy_rules(extra: RequestId, setting_type: &str, rules: Vec<Value>) -> String {
    json!({
        "@type": "setUserPrivacySettingRules",
        "@extra": extra.as_extra(),
        "setting": {"@type": setting_type},
        "rules": {"@type": "userPrivacySettingRules", "rules": rules},
    })
    .to_string()
}

/// Slice S3: `getReadDatePrivacySettings` (schema 1.8.67, :15626 —
/// "Returns privacy settings for message read date in private chats").
pub fn get_read_date_privacy_settings(extra: RequestId) -> String {
    json!({
        "@type": "getReadDatePrivacySettings",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// Slice S3: `setReadDatePrivacySettings` (schema 1.8.67, :15623).
/// `readDatePrivacySettings show_read_date:Bool` (:9026).
pub fn set_read_date_privacy_settings(extra: RequestId, show_read_date: bool) -> String {
    json!({
        "@type": "setReadDatePrivacySettings",
        "@extra": extra.as_extra(),
        "settings": {"@type": "readDatePrivacySettings", "show_read_date": show_read_date},
    })
    .to_string()
}

/// Slice S3: `getBlockedMessageSenders` (schema 1.8.67, :14505 —
/// `getBlockedMessageSenders block_list:BlockList offset:int32
/// limit:int32 = MessageSenders;`). The main block list is
/// `blockListMain` (:9692).
pub fn get_blocked_message_senders(extra: RequestId, offset: i32, limit: i32) -> String {
    json!({
        "@type": "getBlockedMessageSenders",
        "@extra": extra.as_extra(),
        "block_list": {"@type": "blockListMain"},
        "offset": offset,
        "limit": limit,
    })
    .to_string()
}

/// B13: `getNewChatPrivacySettings` (schema 1.8.67, :15632 — "Returns
/// privacy settings for new chats").
pub fn get_new_chat_privacy_settings(extra: RequestId) -> String {
    json!({
        "@type": "getNewChatPrivacySettings",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// B13: `setNewChatPrivacySettings` (schema 1.8.67, :15629).
/// `newChatPrivacySettings allow_new_chats_from_unknown_users:Bool
/// incoming_paid_message_star_count:int53` (:9033). The star count is
/// sent back unchanged (the paid-messages price slider is not part of
/// this screen).
pub fn set_new_chat_privacy_settings(
    extra: RequestId,
    allow_from_unknown: bool,
    incoming_paid_message_star_count: i64,
) -> String {
    json!({
        "@type": "setNewChatPrivacySettings",
        "@extra": extra.as_extra(),
        "settings": {
            "@type": "newChatPrivacySettings",
            "allow_new_chats_from_unknown_users": allow_from_unknown,
            "incoming_paid_message_star_count": incoming_paid_message_star_count,
        },
    })
    .to_string()
}

/// B13: `setGiftSettings` (schema 1.8.67, :15293 — "Changes settings for
/// gift receiving for the current user").
pub fn set_gift_settings(extra: RequestId, settings: Value) -> String {
    json!({
        "@type": "setGiftSettings",
        "@extra": extra.as_extra(),
        "settings": settings,
    })
    .to_string()
}

/// B13: `setInactiveSessionTtl` (schema 1.8.67, :15120 — "Changes the
/// period of inactivity after which sessions will automatically be
/// terminated"). `inactive_session_ttl_days` is 1-366.
pub fn set_inactive_session_ttl(extra: RequestId, days: i32) -> String {
    json!({
        "@type": "setInactiveSessionTtl",
        "@extra": extra.as_extra(),
        "inactive_session_ttl_days": days,
    })
    .to_string()
}

/// B13: `getRecoveryEmailAddress` (schema 1.8.67) — TDLib documents it as
/// the way to verify a password the user typed, which is how the
/// "Do you still remember your password?" check works.
pub fn get_recovery_email_address(extra: RequestId, password: &str) -> String {
    json!({
        "@type": "getRecoveryEmailAddress",
        "@extra": extra.as_extra(),
        "password": password,
    })
    .to_string()
}

/// B13: `hideSuggestedAction` (schema 1.8.67, :12971) for a
/// `suggestedActionCheckPassword`.
pub fn hide_check_password_suggestion(extra: RequestId) -> String {
    json!({
        "@type": "hideSuggestedAction",
        "@extra": extra.as_extra(),
        "action": {"@type": "suggestedActionCheckPassword"},
    })
    .to_string()
}

/// B13: `getNetworkStatistics` (schema 1.8.67, :15808). `only_current`
/// false = everything since the last reset.
pub fn get_network_statistics(extra: RequestId) -> String {
    json!({
        "@type": "getNetworkStatistics",
        "@extra": extra.as_extra(),
        "only_current": false,
    })
    .to_string()
}

/// B13: `resetNetworkStatistics` (schema 1.8.67, :15814).
pub fn reset_network_statistics(extra: RequestId) -> String {
    json!({
        "@type": "resetNetworkStatistics",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privacy::PrivacyRuleDetail;
    use crate::telegram::requests::set_message_sender_block_list;

    #[test]
    fn privacy_requests_match_1_8_67() {
        // Slice S3: what each request shape is ultimately validating —
        // the JSON matches the pinned schema constructors verbatim
        // (:15620, :15617, :15626, :15623, :14505, :14492).
        let v: serde_json::Value = serde_json::from_str(&get_privacy_rules(
            RequestId(1),
            "userPrivacySettingShowStatus",
        ))
        .unwrap();
        assert_eq!(v["@type"], "getUserPrivacySettingRules");
        assert_eq!(v["setting"]["@type"], "userPrivacySettingShowStatus");

        // Exceptions compose in TDLib match order (:8975) and the TGX
        // canonical order (`PrivacySettings.toggleUser`: restrict-users
        // before allow-users): never first, always second, base last.
        let rules = PrivacyRuleDetail {
            who: Some(PrivacyWho::Contacts),
            always: vec![7],
            never: vec![9],
            ..Default::default()
        }
        .recompose();
        let v: serde_json::Value = serde_json::from_str(&set_privacy_rules(
            RequestId(2),
            "userPrivacySettingShowStatus",
            rules,
        ))
        .unwrap();
        assert_eq!(v["@type"], "setUserPrivacySettingRules");
        assert_eq!(v["setting"]["@type"], "userPrivacySettingShowStatus");
        let r = v["rules"]["rules"].as_array().unwrap();
        assert_eq!(r.len(), 3);
        assert_eq!(r[0]["@type"], "userPrivacySettingRuleRestrictUsers");
        assert_eq!(r[0]["user_ids"], serde_json::json!([9]));
        assert_eq!(r[1]["@type"], "userPrivacySettingRuleAllowUsers");
        assert_eq!(r[1]["user_ids"], serde_json::json!([7]));
        assert_eq!(r[2]["@type"], "userPrivacySettingRuleAllowContacts");

        let v: serde_json::Value =
            serde_json::from_str(&get_read_date_privacy_settings(RequestId(3))).unwrap();
        assert_eq!(v["@type"], "getReadDatePrivacySettings");

        let v: serde_json::Value =
            serde_json::from_str(&set_read_date_privacy_settings(RequestId(4), false)).unwrap();
        assert_eq!(v["@type"], "setReadDatePrivacySettings");
        assert_eq!(v["settings"]["@type"], "readDatePrivacySettings");
        assert_eq!(v["settings"]["show_read_date"], false);

        let v: serde_json::Value =
            serde_json::from_str(&get_blocked_message_senders(RequestId(5), 0, 100)).unwrap();
        assert_eq!(v["@type"], "getBlockedMessageSenders");
        assert_eq!(v["block_list"]["@type"], "blockListMain");
        assert_eq!(v["offset"], 0);
        assert_eq!(v["limit"], 100);

        // Unblock passes a null block list (TGX `unblockSender`).
        let v: serde_json::Value =
            serde_json::from_str(&set_message_sender_block_list(RequestId(6), 42, false)).unwrap();
        assert_eq!(v["@type"], "setMessageSenderBlockList");
        assert_eq!(v["sender_id"]["@type"], "messageSenderUser");
        assert_eq!(v["sender_id"]["user_id"], 42);
        assert!(v["block_list"].is_null());
    }
}

#[cfg(test)]
mod b13_tests {
    use super::*;
    use serde_json::json;

    fn parse(json: String) -> Value {
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn new_keys_use_the_schema_constructor_names() {
        let names: Vec<&str> = PrivacySettingKey::all()
            .iter()
            .map(|k| k.td_type())
            .collect();
        for expected in [
            "userPrivacySettingShowBio",
            "userPrivacySettingShowBirthdate",
            "userPrivacySettingShowProfileAudio",
            "userPrivacySettingAllowFindingByPhoneNumber",
            "userPrivacySettingAllowPrivateVoiceAndVideoNoteMessages",
            "userPrivacySettingAutosaveGifts",
        ] {
            assert!(names.contains(&expected), "{expected} missing");
        }
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len());
        // Every key has a place on the screen: the two blocks, plus the
        // find-by-number choice that lives inside the phone number editor.
        assert_eq!(
            PrivacySettingKey::visibility_rows().len()
                + PrivacySettingKey::contact_rows().len()
                + 1,
            names.len()
        );
    }

    #[test]
    fn rule_types_follow_tdesktop_per_key() {
        use PrivacySettingKey as K;
        assert!(K::AllowChatInvites.allows_premium_exception(true));
        assert!(!K::AllowChatInvites.allows_premium_exception(false));
        assert!(!K::ShowStatus.allows_premium_exception(true));
        assert!(K::AutosaveGifts.allows_bots_exception());
        assert!(!K::AllowChatInvites.allows_bots_exception());
        assert!(K::AllowVoiceMessages.restriction_needs_premium());
        assert!(!K::ShowBio.restriction_needs_premium());
        assert_eq!(
            K::AllowFindingByPhoneNumber.options(),
            [PrivacyWho::Everybody, PrivacyWho::Contacts]
        );
        assert!(!K::AllowFindingByPhoneNumber.has_exceptions());
        assert_eq!(K::ShowBirthdate.options().len(), 3);
    }

    #[test]
    fn b13_request_shapes_match_1_8_67() {
        let v = parse(set_new_chat_privacy_settings(RequestId(1), false, 12));
        assert_eq!(v["@type"], "setNewChatPrivacySettings");
        assert_eq!(v["settings"]["@type"], "newChatPrivacySettings");
        assert_eq!(v["settings"]["allow_new_chats_from_unknown_users"], false);
        assert_eq!(v["settings"]["incoming_paid_message_star_count"], 12);
        assert_eq!(
            parse(get_new_chat_privacy_settings(RequestId(2)))["@type"],
            "getNewChatPrivacySettings"
        );
        let v = parse(set_gift_settings(
            RequestId(3),
            json!({"@type": "giftSettings", "show_gift_button": true}),
        ));
        assert_eq!(v["@type"], "setGiftSettings");
        assert_eq!(v["settings"]["show_gift_button"], true);
        let v = parse(set_inactive_session_ttl(RequestId(4), 90));
        assert_eq!(v["@type"], "setInactiveSessionTtl");
        assert_eq!(v["inactive_session_ttl_days"], 90);
        let v = parse(get_network_statistics(RequestId(5)));
        assert_eq!(v["@type"], "getNetworkStatistics");
        assert_eq!(v["only_current"], false);
        assert_eq!(
            parse(reset_network_statistics(RequestId(6)))["@type"],
            "resetNetworkStatistics"
        );
        let v = parse(get_recovery_email_address(RequestId(7), "pw"));
        assert_eq!(v["@type"], "getRecoveryEmailAddress");
        assert_eq!(v["password"], "pw");
        let v = parse(hide_check_password_suggestion(RequestId(8)));
        assert_eq!(v["@type"], "hideSuggestedAction");
        assert_eq!(v["action"]["@type"], "suggestedActionCheckPassword");
    }
}
