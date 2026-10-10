//! Settings → Privacy domain state (TGX `SettingsPrivacyController`):
//! parsed `UserPrivacySettingRule` values, the per-key rule detail with
//! always/never exception lists, and the fetch state the Privacy screen
//! keys "loading / loaded / failed" off. Unknown rule types round-trip
//! untouched via `PrivacyRuleDetail::extra_rules`.

use crate::telegram::requests::PrivacyWho;
use serde_json::Value;

/// Slice S3: one parsed `UserPrivacySettingRule` — the constructor name
/// plus the exception ids it carries (`userPrivacySettingRuleAllowUsers`
/// / `userPrivacySettingRuleRestrictUsers` / the chat-member variants,
/// schema 1.8.67, :8955-:8973).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivacyRule {
    pub name: String,
    pub user_ids: Vec<i64>,
    pub chat_ids: Vec<i64>,
}

impl PrivacyRule {
    pub fn parse(value: &Value) -> Self {
        let ids = |key: &str| {
            value
                .get(key)
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_i64).collect())
                .unwrap_or_default()
        };
        Self {
            name: value
                .get("@type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            user_ids: ids("user_ids"),
            chat_ids: ids("chat_ids"),
        }
    }
}

/// Slice S3 (privacy screen): a parsed `userPrivacySettingRules` answer —
/// the base Everybody / Contacts / Nobody choice plus the always-allow /
/// never-allow exception user lists. Rules that are neither the base
/// choice nor user exceptions (chat-member exceptions, premium/bots
/// rules) are kept verbatim in `extra_rules` and passed through on every
/// recompose — the Quill UI only edits user exceptions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrivacyRuleDetail {
    pub who: Option<PrivacyWho>,
    pub always: Vec<i64>,
    pub never: Vec<i64>,
    /// `userPrivacySettingRuleAllowChatMembers` chat ids (tdesktop adds
    /// groups to the always-allow list).
    pub always_chats: Vec<i64>,
    /// `userPrivacySettingRuleRestrictChatMembers` chat ids.
    pub never_chats: Vec<i64>,
    /// `userPrivacySettingRuleAllowPremiumUsers` (tdesktop "Premium users").
    pub allow_premium: bool,
    /// `userPrivacySettingRuleAllowBots` (tdesktop "Mini Apps", Always).
    pub allow_bots: bool,
    /// `userPrivacySettingRuleRestrictBots` (tdesktop "Mini Apps", Never).
    pub never_bots: bool,
    pub extra_rules: Vec<PrivacyRule>,
}

impl PrivacyRuleDetail {
    /// Slice S3: copy with the base choice replaced. Mirrors TGX
    /// `PrivacySettings.toggleGlobal`: changing the base keeps the
    /// exception rules (and any unknown extras) in place.
    pub fn with_base(&self, who: PrivacyWho) -> Self {
        let mut next = self.clone();
        next.who = Some(who);
        next
    }

    pub fn from_rules(rules: &[PrivacyRule]) -> Self {
        let names: Vec<String> = rules.iter().map(|r| r.name.clone()).collect();
        let mut detail = PrivacyRuleDetail {
            who: PrivacyWho::from_rule_names(&names),
            ..Default::default()
        };
        for rule in rules {
            match rule.name.as_str() {
                "userPrivacySettingRuleAllowUsers" => detail.always.extend(&rule.user_ids),
                "userPrivacySettingRuleRestrictUsers" => detail.never.extend(&rule.user_ids),
                "userPrivacySettingRuleAllowChatMembers" => {
                    detail.always_chats.extend(&rule.chat_ids)
                }
                "userPrivacySettingRuleRestrictChatMembers" => {
                    detail.never_chats.extend(&rule.chat_ids)
                }
                "userPrivacySettingRuleAllowPremiumUsers" => detail.allow_premium = true,
                "userPrivacySettingRuleAllowBots" => detail.allow_bots = true,
                "userPrivacySettingRuleRestrictBots" => detail.never_bots = true,
                "userPrivacySettingRuleAllowAll"
                | "userPrivacySettingRuleRestrictAll"
                | "userPrivacySettingRuleAllowContacts"
                | "userPrivacySettingRuleRestrictContacts" => {}
                _ => detail.extra_rules.push(rule.clone()),
            }
        }
        detail
    }

    /// Slice S3: rebuild the TDLib rule list for a `set` call. TDLib
    /// matches rules in order (schema 1.8.67, :8975 — "The first matched
    /// rule defines the privacy setting") and TGX canonicalizes
    /// restrict-users before allow-users (`PrivacySettings.toggleUser`):
    /// never-exceptions first, always-exceptions, preserved unknown
    /// extras (Premium/bots/chat-member rules the three-option UI can't
    /// represent), then the base rule (TGX `toggleGlobal` appends the
    /// base after the exceptions).
    pub fn recompose(&self) -> Vec<serde_json::Value> {
        use serde_json::json;
        let mut rules: Vec<serde_json::Value> = Vec::new();
        if self.never_bots {
            rules.push(json!({"@type": "userPrivacySettingRuleRestrictBots"}));
        }
        if !self.never.is_empty() {
            rules.push(
                json!({"@type": "userPrivacySettingRuleRestrictUsers", "user_ids": self.never}),
            );
        }
        if !self.never_chats.is_empty() {
            rules.push(
                json!({"@type": "userPrivacySettingRuleRestrictChatMembers", "chat_ids": self.never_chats}),
            );
        }
        if self.allow_premium {
            rules.push(json!({"@type": "userPrivacySettingRuleAllowPremiumUsers"}));
        }
        if self.allow_bots {
            rules.push(json!({"@type": "userPrivacySettingRuleAllowBots"}));
        }
        if !self.always.is_empty() {
            rules.push(
                json!({"@type": "userPrivacySettingRuleAllowUsers", "user_ids": self.always}),
            );
        }
        if !self.always_chats.is_empty() {
            rules.push(
                json!({"@type": "userPrivacySettingRuleAllowChatMembers", "chat_ids": self.always_chats}),
            );
        }
        for extra in &self.extra_rules {
            let mut v = json!({"@type": extra.name});
            if !extra.user_ids.is_empty() {
                v["user_ids"] = json!(extra.user_ids);
            }
            if !extra.chat_ids.is_empty() {
                v["chat_ids"] = json!(extra.chat_ids);
            }
            rules.push(v);
        }
        if let Some(who) = self.who {
            rules.extend(who.rules());
        }
        rules
    }

    /// Exceptions relevant to the base choice (TGX
    /// `PrivacySettings.needNeverAllow` / `needAlwaysAllow`):
    /// Everybody → never-allow only; Contacts → both; Nobody →
    /// always-allow only.
    pub fn exception_counts(&self) -> (usize, usize) {
        let mut always = self.always.len() + self.always_chats.len();
        let mut never = self.never.len() + self.never_chats.len();
        always += usize::from(self.allow_premium) + usize::from(self.allow_bots);
        never += usize::from(self.never_bots);
        match self.who {
            Some(PrivacyWho::Everybody) => always = 0,
            Some(PrivacyWho::Nobody) => never = 0,
            _ => {}
        }
        (always, never)
    }
}

/// Slice S3: fetch state for one privacy rule key — the Privacy screen
/// keys "still loading" vs "loaded" vs "failed" off this, never off the
/// rule map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivacyKeyState {
    Loading,
    Ready(PrivacyRuleDetail),
    Failed,
}

/// B13: `giftSettings` (schema 1.8.67, :1445) — whether the gift button
/// shows in chats and which gift kinds the account accepts
/// (`acceptedGiftTypes`, :1440). tdesktop shows the same six switches in
/// Gifts privacy; changing them needs Premium.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GiftSettings {
    pub show_gift_button: bool,
    pub unlimited_gifts: bool,
    pub limited_gifts: bool,
    pub upgraded_gifts: bool,
    pub gifts_from_channels: bool,
    pub premium_subscription: bool,
}

impl Default for GiftSettings {
    /// A fresh account accepts everything.
    fn default() -> Self {
        Self {
            show_gift_button: false,
            unlimited_gifts: true,
            limited_gifts: true,
            upgraded_gifts: true,
            gifts_from_channels: true,
            premium_subscription: true,
        }
    }
}

impl GiftSettings {
    /// Parse a `giftSettings` object (`None` for a null/absent field).
    pub fn from_value(value: Option<&Value>) -> Option<Self> {
        let value = value.filter(|v| v.is_object())?;
        let flag = |v: &Value, key: &str| v.get(key).and_then(Value::as_bool).unwrap_or(false);
        let accepted = value.get("accepted_gift_types").unwrap_or(&Value::Null);
        Some(Self {
            show_gift_button: flag(value, "show_gift_button"),
            unlimited_gifts: flag(accepted, "unlimited_gifts"),
            limited_gifts: flag(accepted, "limited_gifts"),
            upgraded_gifts: flag(accepted, "upgraded_gifts"),
            gifts_from_channels: flag(accepted, "gifts_from_channels"),
            premium_subscription: flag(accepted, "premium_subscription"),
        })
    }

    /// The `giftSettings` object for `setGiftSettings`.
    pub fn to_value(self) -> Value {
        serde_json::json!({
            "@type": "giftSettings",
            "show_gift_button": self.show_gift_button,
            "accepted_gift_types": {
                "@type": "acceptedGiftTypes",
                "unlimited_gifts": self.unlimited_gifts,
                "limited_gifts": self.limited_gifts,
                "upgraded_gifts": self.upgraded_gifts,
                "gifts_from_channels": self.gifts_from_channels,
                "premium_subscription": self.premium_subscription,
            },
        })
    }
}

/// B13: `newChatPrivacySettings` (schema 1.8.67, :9033) — tdesktop's
/// Messages privacy box: "Everyone" or "Contacts and Premium users"
/// (`allow_new_chats_from_unknown_users` off), plus the paid-message
/// price kept as-is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewChatPrivacy {
    pub allow_from_unknown: bool,
    pub incoming_paid_message_star_count: i64,
}

impl NewChatPrivacy {
    pub fn from_value(value: &Value) -> Self {
        Self {
            allow_from_unknown: value
                .get("allow_new_chats_from_unknown_users")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            incoming_paid_message_star_count: value
                .get("incoming_paid_message_star_count")
                .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
                .unwrap_or(0),
        }
    }
}

/// B13: the inactivity periods tdesktop offers for terminating old
/// sessions (`SelfDestructionBox` `Type::Sessions`): 1 week, then 1, 3, 6
/// and 12 months, as days.
pub const SESSION_TTL_DAYS: [i32; 5] = [7, 30, 90, 180, 365];

/// B13: tdesktop `SelfDestructionBox::DaysLabel` — whole months above 25
/// days, else whole weeks.
pub fn session_ttl_label(days: i32) -> String {
    let (count, unit) = if days > 25 {
        ((days / 30).max(1), "month")
    } else {
        ((days / 7).max(1), "week")
    };
    if count == 1 {
        format!("1 {unit}")
    } else {
        format!("{count} {unit}s")
    }
}

/// B13: fetch state of the new-chat privacy row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewChatPrivacyState {
    Loading,
    Ready(NewChatPrivacy),
    Failed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{DiagnosticSink, MemorySink};
    use crate::ids::AccountKey;
    use crate::state::SettingsPurpose;
    use crate::state::{RequestPurpose, Session};
    use crate::telegram::client::copy_and_parse;
    use crate::telegram::requests_privacy::PrivacySettingKey;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;

    fn session() -> (Session, Arc<MemorySink>) {
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        (Session::new(AccountKey::primary(), dyn_sink), sink)
    }

    fn apply_json(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>, json: &str) {
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let owned = copy_and_parse(json, seq, &dyn_sink).unwrap();
        session.apply(owned);
    }

    #[test]
    fn privacy_get_maps_rules_with_exceptions() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(
            RequestPurpose::Settings(SettingsPurpose::GetPrivacyRules {
                key: PrivacySettingKey::ShowStatus,
            }),
            None,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"userPrivacySettingRules","@extra":"{}","rules":[{{"@type":"userPrivacySettingRuleRestrictUsers","user_ids":[9]}},{{"@type":"userPrivacySettingRuleAllowUsers","user_ids":[7]}},{{"@type":"userPrivacySettingRuleAllowPremium"}},{{"@type":"userPrivacySettingRuleAllowContacts"}}]}}"#,
                extra.0,
            ),
        );
        let PrivacyKeyState::Ready(detail) = &session.privacy[&PrivacySettingKey::ShowStatus]
        else {
            panic!("expected Ready");
        };
        assert_eq!(detail.who, Some(PrivacyWho::Contacts));
        assert_eq!(detail.always, vec![7]);
        assert_eq!(detail.never, vec![9]);
        assert_eq!(detail.extra_rules.len(), 1);
        assert_eq!(
            detail.extra_rules[0].name,
            "userPrivacySettingRuleAllowPremium"
        );
        // Recompose keeps TGX canonical order: never, always, extras,
        // base (schema 1.8.67, :8975).
        let rules = detail.recompose();
        let names: Vec<&str> = rules
            .iter()
            .map(|r| r.get("@type").and_then(|t| t.as_str()).unwrap_or(""))
            .collect();
        assert_eq!(
            names,
            [
                "userPrivacySettingRuleRestrictUsers",
                "userPrivacySettingRuleAllowUsers",
                "userPrivacySettingRuleAllowPremium",
                "userPrivacySettingRuleAllowContacts",
            ]
        );
    }

    /// Slice S3: a failed privacy `set` marks the key Failed so the UI
    /// shows it instead of the stale optimistic value.
    #[test]
    fn privacy_set_failure_marks_failed() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.privacy.insert(
            PrivacySettingKey::ShowPhoneNumber,
            PrivacyKeyState::Ready(PrivacyRuleDetail {
                who: Some(PrivacyWho::Nobody),
                ..Default::default()
            }),
        );
        let extra = session.request(
            RequestPurpose::Settings(SettingsPurpose::SetPrivacyRules {
                key: PrivacySettingKey::ShowPhoneNumber,
            }),
            None,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"PRIVACY_TOO_LONG"}}"#,
                extra.0,
            ),
        );
        assert_eq!(
            session.privacy[&PrivacySettingKey::ShowPhoneNumber],
            PrivacyKeyState::Failed
        );
    }

    /// Slice S3: `updateUserPrivacySettingRules` (another device) refreshes
    /// the matching key; unknown settings are ignored.
    #[test]
    fn privacy_live_update_refreshes_key() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateUserPrivacySettingRules","setting":{"@type":"userPrivacySettingShowStatus"},"rules":{"@type":"userPrivacySettingRules","rules":[{"@type":"userPrivacySettingRuleAllowAll"}]}}"#,
        );
        let PrivacyKeyState::Ready(detail) = &session.privacy[&PrivacySettingKey::ShowStatus]
        else {
            panic!("expected Ready");
        };
        assert_eq!(detail.who, Some(PrivacyWho::Everybody));
        // Unknown setting names must not touch the map.
        apply_json(
            &mut session,
            &seq,
            &sink,
            r#"{"@type":"updateUserPrivacySettingRules","setting":{"@type":"userPrivacySettingNope"},"rules":{"@type":"userPrivacySettingRules","rules":[]}}"#,
        );
        assert_eq!(session.privacy.len(), 1);
    }

    /// Slice S3: the read-date roundtrip — `get` stores the value and
    /// clears loading; a failed `set` flags the error and drops the
    /// optimistic value.
    #[test]
    fn read_date_get_and_set_failure() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        session.read_date_loading = true;
        let extra = session.request(RequestPurpose::GetReadDatePrivacy, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"readDatePrivacySettings","@extra":"{}","show_read_date":false}}"#,
                extra.0,
            ),
        );
        assert_eq!(session.read_date_show, Some(false));
        assert!(!session.read_date_loading);
        assert!(!session.read_date_error);

        session.read_date_show = Some(true); // optimistic set
        let extra = session.request(RequestPurpose::SetReadDatePrivacy, None);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"BAD"}}"#,
                extra.0,
            ),
        );
        assert!(session.read_date_error);
        assert!(!session.read_date_loading);
        assert_eq!(session.read_date_show, None);
    }

    /// Slice S3: blocked-senders paging — page 0 replaces, later pages
    /// append; `total_count` tracks the server total.
    #[test]
    fn blocked_senders_pages_append() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(
            RequestPurpose::Settings(SettingsPurpose::GetBlockedSenders { offset: 0 }),
            None,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"messageSenders","@extra":"{}","total_count":3,"senders":[{{"@type":"messageSenderUser","user_id":61}},{{"@type":"messageSenderUser","user_id":62}}]}}"#,
                extra.0,
            ),
        );
        assert_eq!(session.blocked_senders, Some(vec![61, 62]));
        assert_eq!(session.blocked_total, 3);
        assert!(!session.blocked_loading);

        let extra = session.request(
            RequestPurpose::Settings(SettingsPurpose::GetBlockedSenders { offset: 2 }),
            None,
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"messageSenders","@extra":"{}","total_count":3,"senders":[{{"@type":"messageSenderUser","user_id":63}}]}}"#,
                extra.0,
            ),
        );
        assert_eq!(session.blocked_senders, Some(vec![61, 62, 63]));
    }
}

#[cfg(test)]
mod b13_tests {
    use super::*;
    use serde_json::json;

    fn rules(list: serde_json::Value) -> Vec<PrivacyRule> {
        list.as_array()
            .unwrap()
            .iter()
            .map(PrivacyRule::parse)
            .collect()
    }

    #[test]
    fn premium_bots_and_chat_member_rules_round_trip() {
        let parsed = rules(json!([
            {"@type": "userPrivacySettingRuleRestrictBots"},
            {"@type": "userPrivacySettingRuleRestrictUsers", "user_ids": [9]},
            {"@type": "userPrivacySettingRuleRestrictChatMembers", "chat_ids": [-100]},
            {"@type": "userPrivacySettingRuleAllowPremiumUsers"},
            {"@type": "userPrivacySettingRuleAllowUsers", "user_ids": [7]},
            {"@type": "userPrivacySettingRuleAllowChatMembers", "chat_ids": [-200, -201]},
            {"@type": "userPrivacySettingRuleAllowContacts"},
        ]));
        let detail = PrivacyRuleDetail::from_rules(&parsed);
        assert_eq!(detail.who, Some(PrivacyWho::Contacts));
        assert!(detail.never_bots && detail.allow_premium && !detail.allow_bots);
        assert_eq!(detail.never, vec![9]);
        assert_eq!(detail.always, vec![7]);
        assert_eq!(detail.never_chats, vec![-100]);
        assert_eq!(detail.always_chats, vec![-200, -201]);
        // Nothing was parked in the pass-through list.
        assert!(detail.extra_rules.is_empty());
        // Contacts mode counts every exception row.
        assert_eq!(detail.exception_counts(), (1 + 2 + 1, 1 + 1 + 1));
        // Recompose keeps restrict rules ahead of allow rules, base last.
        let again = detail.recompose();
        let names: Vec<&str> = again.iter().map(|r| r["@type"].as_str().unwrap()).collect();
        assert_eq!(
            names,
            [
                "userPrivacySettingRuleRestrictBots",
                "userPrivacySettingRuleRestrictUsers",
                "userPrivacySettingRuleRestrictChatMembers",
                "userPrivacySettingRuleAllowPremiumUsers",
                "userPrivacySettingRuleAllowUsers",
                "userPrivacySettingRuleAllowChatMembers",
                "userPrivacySettingRuleAllowContacts",
            ]
        );
        let reparsed: Vec<PrivacyRule> = again.iter().map(PrivacyRule::parse).collect();
        assert_eq!(PrivacyRuleDetail::from_rules(&reparsed), detail);
    }

    #[test]
    fn exception_counts_follow_the_base_choice() {
        let mut detail = PrivacyRuleDetail {
            who: Some(PrivacyWho::Everybody),
            allow_premium: true,
            always: vec![1],
            never_chats: vec![5],
            ..Default::default()
        };
        // Everybody: only the never list matters.
        assert_eq!(detail.exception_counts(), (0, 1));
        detail.who = Some(PrivacyWho::Nobody);
        assert_eq!(detail.exception_counts(), (2, 0));
    }

    #[test]
    fn gift_settings_parse_and_serialize() {
        let value = json!({
            "@type": "giftSettings",
            "show_gift_button": true,
            "accepted_gift_types": {
                "@type": "acceptedGiftTypes",
                "unlimited_gifts": true, "limited_gifts": false, "upgraded_gifts": true,
                "gifts_from_channels": false, "premium_subscription": true
            }
        });
        let settings = GiftSettings::from_value(Some(&value)).unwrap();
        assert!(settings.show_gift_button && settings.unlimited_gifts);
        assert!(!settings.limited_gifts && !settings.gifts_from_channels);
        assert_eq!(settings.to_value(), value);
        assert!(GiftSettings::from_value(Some(&serde_json::Value::Null)).is_none());
        assert!(GiftSettings::from_value(None).is_none());
        // A fresh account accepts every kind and hides the gift button.
        let fresh = GiftSettings::default();
        assert!(!fresh.show_gift_button && fresh.limited_gifts && fresh.premium_subscription);
    }

    #[test]
    fn new_chat_privacy_defaults_to_everybody() {
        let parsed = NewChatPrivacy::from_value(&json!({
            "allow_new_chats_from_unknown_users": false,
            "incoming_paid_message_star_count": "40"
        }));
        assert!(!parsed.allow_from_unknown);
        assert_eq!(parsed.incoming_paid_message_star_count, 40);
        assert!(NewChatPrivacy::from_value(&json!({})).allow_from_unknown);
    }

    #[test]
    fn session_ttl_labels_follow_tdesktop() {
        let labels: Vec<String> = SESSION_TTL_DAYS
            .iter()
            .map(|d| session_ttl_label(*d))
            .collect();
        assert_eq!(
            labels,
            ["1 week", "1 month", "3 months", "6 months", "12 months"]
        );
        assert_eq!(session_ttl_label(14), "2 weeks");
        assert_eq!(session_ttl_label(1), "1 week");
        assert_eq!(session_ttl_label(366), "12 months");
    }
}
