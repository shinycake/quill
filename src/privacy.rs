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
        if !self.never.is_empty() {
            rules.push(
                json!({"@type": "userPrivacySettingRuleRestrictUsers", "user_ids": self.never}),
            );
        }
        if !self.always.is_empty() {
            rules.push(
                json!({"@type": "userPrivacySettingRuleAllowUsers", "user_ids": self.always}),
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
        let (mut always, mut never) = (self.always.len(), self.never.len());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{DiagnosticSink, MemorySink};
    use crate::ids::AccountKey;
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
            RequestPurpose::GetPrivacyRules {
                key: PrivacySettingKey::ShowStatus,
            },
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
            RequestPurpose::SetPrivacyRules {
                key: PrivacySettingKey::ShowPhoneNumber,
            },
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
        let extra = session.request(RequestPurpose::GetBlockedSenders { offset: 0 }, None);
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

        let extra = session.request(RequestPurpose::GetBlockedSenders { offset: 2 }, None);
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
