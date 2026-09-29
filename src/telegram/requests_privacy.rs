//! Slice S3 (privacy screen): `UserPrivacySetting` request builders —
//! `getUserPrivacySettingRules` / `setUserPrivacySettingRules` for the five
//! screen rules, `getReadDatePrivacySettings` /
//! `setReadDatePrivacySettings` for the read-date toggle, and
//! `getBlockedMessageSenders` for the blocked-users list. Blocking itself
//! reuses the CL3 `setMessageSenderBlockList` (passing a null block list
//! unblocks, TGX `Tdlib.unblockSender`).

use crate::ids::RequestId;
use serde_json::{Value, json};

/// Slice S3 (privacy screen): the `UserPrivacySetting` constructors the
/// Privacy screen edits (schema 1.8.67, :8981-:9003). Call settings keep
/// the pre-existing `CallPrivacySetting` / `call_privacy_*` plumbing
/// (Phase C2i) — the screen reads those fields directly instead of
/// re-plumbing them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrivacySettingKey {
    ShowStatus,
    ShowPhoneNumber,
    ShowProfilePhoto,
    ShowLinkInForwardedMessages,
    AllowChatInvites,
}

impl PrivacySettingKey {
    /// Verbatim `UserPrivacySetting` constructor names (schema 1.8.67,
    /// :8982 / :8991 / :8985 / :8988 / :9003).
    pub fn td_type(self) -> &'static str {
        match self {
            PrivacySettingKey::ShowStatus => "userPrivacySettingShowStatus",
            PrivacySettingKey::ShowPhoneNumber => "userPrivacySettingShowPhoneNumber",
            PrivacySettingKey::ShowProfilePhoto => "userPrivacySettingShowProfilePhoto",
            PrivacySettingKey::ShowLinkInForwardedMessages => {
                "userPrivacySettingShowLinkInForwardedMessages"
            }
            PrivacySettingKey::AllowChatInvites => "userPrivacySettingAllowChatInvites",
        }
    }

    /// Screen row labels (TGX `SettingsPrivacyKeyController.getName`
    /// strings: LastSeen / PhoneNumber / PrivacyPhotoTitle /
    /// PrivacyForwardLinkTitle / GroupsAndChannels).
    pub fn label(self) -> &'static str {
        match self {
            PrivacySettingKey::ShowStatus => "Last Seen & Online",
            PrivacySettingKey::ShowPhoneNumber => "Phone Number",
            PrivacySettingKey::ShowProfilePhoto => "Profile Photos",
            PrivacySettingKey::ShowLinkInForwardedMessages => "Forwarded Messages",
            PrivacySettingKey::AllowChatInvites => "Groups & Channels",
        }
    }

    /// Slice S3: the five privacy-screen rules (TGX
    /// `SettingsPrivacyController`). The two call settings
    /// (`userPrivacySettingAllowCalls`,
    /// `userPrivacySettingAllowPeerToPeerCalls`) keep their Phase C2i
    /// plumbing and are edited separately.
    pub const fn all() -> [PrivacySettingKey; 5] {
        [
            PrivacySettingKey::ShowStatus,
            PrivacySettingKey::ShowPhoneNumber,
            PrivacySettingKey::ShowProfilePhoto,
            PrivacySettingKey::ShowLinkInForwardedMessages,
            PrivacySettingKey::AllowChatInvites,
        ]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privacy::PrivacyRuleDetail;
    use crate::telegram::requests::PrivacyWho;
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
