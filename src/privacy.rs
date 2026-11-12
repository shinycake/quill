//! Settings → Privacy domain state (TGX `SettingsPrivacyController`):
//! parsed `UserPrivacySettingRule` values, the per-key rule detail with
//! always/never exception lists, and the fetch state the Privacy screen
//! keys "loading / loaded / failed" off. Unknown rule types round-trip
//! untouched via `PrivacyRuleDetail::extra_rules`.
#![deny(clippy::indexing_slicing)]

pub mod prefs;

use crate::ids::RequestId;
use crate::telegram::requests::PrivacyWho;
use crate::telegram::requests_privacy::PrivacySettingKey;
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
                crate::json_access::set(&mut v, "user_ids", json!(extra.user_ids));
            }
            if !extra.chat_ids.is_empty() {
                crate::json_access::set(&mut v, "chat_ids", json!(extra.chat_ids));
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
/// rule map. A request in the state carries its id, and its answer applies
/// only while that id is still the key's live request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivacyKeyState {
    /// `getUserPrivacySettingRules` is out; `stale` is the rule held before
    /// the refetch (shown meanwhile and kept if the refetch fails).
    Loading {
        request: RequestId,
        stale: Option<PrivacyRuleDetail>,
    },
    Ready(PrivacyRuleDetail),
    /// `setUserPrivacySettingRules` is out: `detail` shows at once (the
    /// live save, under `request`). `confirmed` is the last rule the
    /// server is known to hold (the one before the first unanswered save,
    /// moved forward as an older save's `ok` arrives) and comes back if the
    /// live save is refused. `earlier` are the older saves still out.
    Saving {
        detail: PrivacyRuleDetail,
        confirmed: PrivacyRuleDetail,
        request: RequestId,
        earlier: Vec<(RequestId, PrivacyRuleDetail)>,
    },
    /// The last save was refused; `detail` is what the server kept, so the
    /// rule stays editable and the editor shows `error`.
    SaveFailed {
        detail: PrivacyRuleDetail,
        error: String,
    },
    /// The rule could not be loaded; `stale` is the rule held before.
    Failed {
        stale: Option<PrivacyRuleDetail>,
    },
}

impl PrivacyKeyState {
    /// The rule as it shows now: settled, being saved, or kept after a
    /// refused save.
    pub fn detail(&self) -> Option<&PrivacyRuleDetail> {
        match self {
            Self::Ready(detail) | Self::Saving { detail, .. } | Self::SaveFailed { detail, .. } => {
                Some(detail)
            }
            Self::Loading { stale, .. } | Self::Failed { stale } => stale.as_ref(),
        }
    }

    /// The rule an edit may be composed on: one the server is known to
    /// hold or that this device is saving. A rule being reloaded or that
    /// failed to load is only the stale copy shown meanwhile; an edit built
    /// on it would overwrite a newer rule from another device.
    pub fn editable_detail(&self) -> Option<&PrivacyRuleDetail> {
        match self {
            Self::Ready(detail) | Self::Saving { detail, .. } | Self::SaveFailed { detail, .. } => {
                Some(detail)
            }
            Self::Loading { .. } | Self::Failed { .. } => None,
        }
    }

    /// The refusal of the last save, until the next edit.
    pub fn save_error(&self) -> Option<&str> {
        match self {
            Self::SaveFailed { error, .. } => Some(error),
            _ => None,
        }
    }

    /// The load or save request that decides this state's next answer.
    fn request(&self) -> Option<RequestId> {
        match self {
            Self::Loading { request, .. } | Self::Saving { request, .. } => Some(*request),
            _ => None,
        }
    }
}

impl crate::state::SettingsState {
    /// A load answer for `key` applies only while it is the key's live
    /// request: a save or an update since then made it stale.
    pub fn finish_privacy_load(
        &mut self,
        key: PrivacySettingKey,
        request: Option<RequestId>,
        detail: PrivacyRuleDetail,
    ) -> bool {
        let live = matches!(self.privacy.get(&key), Some(PrivacyKeyState::Loading { request: id, .. }) if Some(*id) == request);
        if live {
            self.privacy.insert(key, PrivacyKeyState::Ready(detail));
        }
        live
    }

    /// A save answered `ok`. The live save's rule is the server's now; an
    /// older save's `ok` moves `confirmed` up to that save's rule (and past
    /// every save sent before it).
    pub fn finish_privacy_save(&mut self, key: PrivacySettingKey, request: Option<RequestId>) {
        let Some(PrivacyKeyState::Saving {
            detail,
            confirmed,
            request: live,
            earlier,
        }) = self.privacy.get_mut(&key)
        else {
            return;
        };
        if Some(*live) == request {
            let detail = detail.clone();
            self.privacy.insert(key, PrivacyKeyState::Ready(detail));
        } else if let Some(at) = earlier.iter().position(|(id, _)| Some(*id) == request)
            && let Some((_, accepted)) = earlier.drain(..=at).next_back()
        {
            *confirmed = accepted;
        }
    }

    /// A save was refused. The live save's refusal puts the key back to
    /// `confirmed`, what the server kept, and says why; it returns the
    /// detail the key shows now (the call rules mirror it). An older save's
    /// refusal only forgets that save: the live one is still out.
    pub fn fail_privacy_save(
        &mut self,
        key: PrivacySettingKey,
        request: Option<RequestId>,
        error: String,
    ) -> Option<PrivacyRuleDetail> {
        let Some(PrivacyKeyState::Saving {
            confirmed,
            request: live,
            earlier,
            ..
        }) = self.privacy.get_mut(&key)
        else {
            return None;
        };
        if Some(*live) != request {
            earlier.retain(|(id, _)| Some(*id) != request);
            return None;
        }
        let detail = confirmed.clone();
        self.privacy.insert(
            key,
            PrivacyKeyState::SaveFailed {
                detail: detail.clone(),
                error,
            },
        );
        Some(detail)
    }

    /// An edit goes out: the new rule shows at once. A save already in
    /// flight stays in `earlier` and the confirmed rule is kept, so a
    /// refusal restores what the server holds, never an edit it has not
    /// accepted.
    pub fn begin_privacy_save(
        &mut self,
        key: PrivacySettingKey,
        detail: PrivacyRuleDetail,
        request: RequestId,
    ) {
        let (confirmed, earlier) = match self.privacy.remove(&key) {
            Some(PrivacyKeyState::Saving {
                detail: older,
                confirmed,
                request: older_request,
                mut earlier,
            }) => {
                earlier.push((older_request, older));
                (confirmed, earlier)
            }
            Some(state) => (
                state.detail().cloned().unwrap_or_else(|| detail.clone()),
                Vec::new(),
            ),
            None => (detail.clone(), Vec::new()),
        };
        self.privacy.insert(
            key,
            PrivacyKeyState::Saving {
                detail,
                confirmed,
                request,
                earlier,
            },
        );
    }

    /// A load goes out. A rule being saved, or already loading, is left
    /// alone: its own answer settles it.
    pub fn begin_privacy_load(&mut self, key: PrivacySettingKey, request: RequestId) -> bool {
        if self
            .privacy
            .get(&key)
            .is_some_and(|s| s.request().is_some())
        {
            return false;
        }
        let stale = self
            .privacy
            .get(&key)
            .and_then(PrivacyKeyState::detail)
            .cloned();
        self.privacy
            .insert(key, PrivacyKeyState::Loading { request, stale });
        true
    }

    /// The load under `request` was refused or never left: only the key's
    /// live load fails, and the rule held before it stays.
    pub fn fail_privacy_load(&mut self, key: PrivacySettingKey, request: Option<RequestId>) {
        let Some(PrivacyKeyState::Loading {
            request: live,
            stale,
        }) = self.privacy.get_mut(&key)
        else {
            return;
        };
        if Some(*live) != request {
            return;
        }
        let stale = stale.take();
        self.privacy.insert(key, PrivacyKeyState::Failed { stale });
    }
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewChatPrivacyState {
    /// The first fetch is out; only its answer (or error) leaves the stage.
    Loading(RequestId),
    Ready(NewChatPrivacy),
    /// Saves are out over the confirmed value; the row shows the newest.
    Saving(crate::state::saves::Saves<NewChatPrivacy>),
    Failed,
}

impl NewChatPrivacyState {
    /// What the row shows, once a value is known.
    pub fn value(&self) -> Option<NewChatPrivacy> {
        match self {
            Self::Ready(value) => Some(*value),
            Self::Saving(saves) => Some(saves.value()),
            Self::Loading(_) | Self::Failed => None,
        }
    }

    /// The stage a row draws: saves in flight show as the value they ask for.
    pub fn shown(&self) -> Self {
        match self.value() {
            Some(value) => Self::Ready(value),
            None => self.clone(),
        }
    }

    /// A save goes out under `request` over the value shown.
    pub fn save(&mut self, request: RequestId, next: NewChatPrivacy) {
        match self {
            Self::Ready(confirmed) => {
                *self = Self::Saving(crate::state::saves::Saves::first(*confirmed, request, next));
            }
            Self::Saving(saves) => saves.push(request, next),
            Self::Loading(_) | Self::Failed => {}
        }
    }

    /// `request` was refused: its value goes, the row falls back.
    pub fn save_failed(&mut self, request: RequestId) {
        if let Self::Saving(saves) = self
            && !saves.fail(request)
        {
            *self = Self::Ready(saves.confirmed());
        }
    }

    /// `request` landed: the server holds its value.
    pub fn save_landed(&mut self, request: RequestId) {
        if let Self::Saving(saves) = self
            && !saves.land(request)
        {
            *self = Self::Ready(saves.confirmed());
        }
    }

    /// The fetch `fetch` answered `server`: it replaces what is shown
    /// unless a save asked after it is still out. The first fetch of this
    /// stage always lands.
    pub fn fetched(&mut self, fetch: RequestId, server: NewChatPrivacy) {
        match self {
            Self::Loading(live) if *live == fetch => *self = Self::Ready(server),
            Self::Ready(_) => *self = Self::Ready(server),
            Self::Saving(saves) => {
                if let Some(value) = saves.fetched(fetch, server) {
                    *self = Self::Ready(value);
                }
            }
            Self::Loading(_) | Self::Failed => {}
        }
    }
}

/// The own gift settings' saves in flight. The settings shown live in the
/// own full info (written at once when a save goes out); this holds what
/// the server is known to have, so a refusal puts that back.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GiftSave {
    #[default]
    Idle,
    Saving(crate::state::saves::Saves<GiftSettings>),
}

impl GiftSave {
    /// A save goes out under `request` over `shown`, the value on screen.
    pub fn save(&mut self, shown: GiftSettings, request: RequestId, next: GiftSettings) {
        match self {
            Self::Idle => {
                *self = Self::Saving(crate::state::saves::Saves::first(shown, request, next));
            }
            Self::Saving(saves) => saves.push(request, next),
        }
    }

    /// `request` was refused: the value to show now (the next save still
    /// out, else the confirmed one), whatever order the saves fail in.
    pub fn failed(&mut self, request: RequestId) -> Option<GiftSettings> {
        let Self::Saving(saves) = self else {
            return None;
        };
        if saves.fail(request) {
            return Some(saves.value());
        }
        let confirmed = saves.confirmed();
        *self = Self::Idle;
        Some(confirmed)
    }

    /// `request` landed: the server holds its value.
    pub fn landed(&mut self, request: RequestId) {
        if let Self::Saving(saves) = self
            && !saves.land(request)
        {
            *self = Self::Idle;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{DiagnosticSink, MemorySink};
    use crate::ids::AccountKey;
    use crate::state::SettingsPurpose;
    use crate::state::{ReadDate, RequestPurpose, Session};
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
        session
            .settings
            .begin_privacy_load(PrivacySettingKey::ShowStatus, extra);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"userPrivacySettingRules","@extra":"{}","rules":[{{"@type":"userPrivacySettingRuleRestrictUsers","user_ids":[9]}},{{"@type":"userPrivacySettingRuleAllowUsers","user_ids":[7]}},{{"@type":"userPrivacySettingRuleAllowPremium"}},{{"@type":"userPrivacySettingRuleAllowContacts"}}]}}"#,
                extra.0,
            ),
        );
        let PrivacyKeyState::Ready(detail) =
            &session.settings.privacy[&PrivacySettingKey::ShowStatus]
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

    fn error_json(extra: RequestId) -> String {
        format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"PRIVACY_TOO_LONG"}}"#,
            extra.0,
        )
    }

    fn who(who: PrivacyWho) -> PrivacyRuleDetail {
        PrivacyRuleDetail {
            who: Some(who),
            ..Default::default()
        }
    }

    /// Sweep 04-046, 12-013, 04-045: a refused save puts back what the
    /// server kept, keeps the rule editable and says why.
    #[test]
    fn privacy_set_failure_restores_the_kept_rule_and_stays_editable() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let key = PrivacySettingKey::AllowCalls;
        session
            .settings
            .privacy
            .insert(key, PrivacyKeyState::Ready(who(PrivacyWho::Nobody)));
        let extra = session.request(
            RequestPurpose::Settings(SettingsPurpose::SetPrivacyRules { key }),
            None,
        );
        session
            .settings
            .begin_privacy_save(key, who(PrivacyWho::Everybody), extra);
        apply_json(&mut session, &seq, &sink, &error_json(extra));
        let state = &session.settings.privacy[&key];
        assert_eq!(state.detail(), Some(&who(PrivacyWho::Nobody)));
        assert!(state.save_error().is_some());
    }

    /// Sweep r3-11, 20-003: a rule that is reloading or failed to load
    /// shows its stale copy but is not a base for an edit.
    #[test]
    fn a_stale_privacy_rule_is_shown_but_not_editable() {
        let (mut session, _sink) = session();
        let key = PrivacySettingKey::ShowStatus;
        let request = session.request(
            RequestPurpose::Settings(SettingsPurpose::SetPrivacyRules { key }),
            None,
        );
        let stale = Some(who(PrivacyWho::Nobody));
        let loading = PrivacyKeyState::Loading {
            request,
            stale: stale.clone(),
        };
        assert_eq!(loading.detail(), stale.as_ref());
        assert_eq!(loading.editable_detail(), None);
        let failed = PrivacyKeyState::Failed { stale };
        assert!(failed.detail().is_some());
        assert_eq!(failed.editable_detail(), None);
        let ready = PrivacyKeyState::Ready(who(PrivacyWho::Nobody));
        assert_eq!(ready.editable_detail(), ready.detail());
    }

    /// Two saves in flight: the first one's refusal does not touch the
    /// second one's value, and the second one's `ok` settles it.
    #[test]
    fn an_older_privacy_save_does_not_decide_the_newer_one() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let key = PrivacySettingKey::ShowStatus;
        session
            .settings
            .privacy
            .insert(key, PrivacyKeyState::Ready(who(PrivacyWho::Nobody)));
        let purpose = RequestPurpose::Settings(SettingsPurpose::SetPrivacyRules { key });
        let first = session.request(purpose, None);
        session
            .settings
            .begin_privacy_save(key, who(PrivacyWho::Contacts), first);
        let second = session.request(purpose, None);
        session
            .settings
            .begin_privacy_save(key, who(PrivacyWho::Everybody), second);
        apply_json(&mut session, &seq, &sink, &error_json(first));
        assert_eq!(
            session.settings.privacy[&key].detail(),
            Some(&who(PrivacyWho::Everybody))
        );
        assert!(matches!(
            session.settings.privacy[&key],
            PrivacyKeyState::Saving { .. }
        ));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, second.0),
        );
        assert_eq!(
            session.settings.privacy[&key],
            PrivacyKeyState::Ready(who(PrivacyWho::Everybody))
        );
    }

    /// Two quick edits, both refused: the key goes back to what the server
    /// kept, not to the first edit (04-046). An accepted first edit is the
    /// rule the second one's refusal restores.
    #[test]
    fn a_refused_second_save_restores_what_the_server_kept() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let key = PrivacySettingKey::ShowStatus;
        let purpose = RequestPurpose::Settings(SettingsPurpose::SetPrivacyRules { key });
        for first_accepted in [false, true] {
            session
                .settings
                .privacy
                .insert(key, PrivacyKeyState::Ready(who(PrivacyWho::Nobody)));
            let first = session.request(purpose, None);
            session
                .settings
                .begin_privacy_save(key, who(PrivacyWho::Contacts), first);
            let second = session.request(purpose, None);
            session
                .settings
                .begin_privacy_save(key, who(PrivacyWho::Everybody), second);
            if first_accepted {
                apply_json(
                    &mut session,
                    &seq,
                    &sink,
                    &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, first.0),
                );
            } else {
                apply_json(&mut session, &seq, &sink, &error_json(first));
            }
            apply_json(&mut session, &seq, &sink, &error_json(second));
            let kept = if first_accepted {
                PrivacyWho::Contacts
            } else {
                PrivacyWho::Nobody
            };
            assert_eq!(session.settings.privacy[&key].detail(), Some(&who(kept)));
            assert!(session.settings.privacy[&key].save_error().is_some());
        }
    }

    /// A late `getUserPrivacySettingRules` answer never overwrites a newer
    /// optimistic value (04-046).
    #[test]
    fn a_late_privacy_load_does_not_overwrite_a_newer_save() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let key = PrivacySettingKey::ShowStatus;
        let load = session.request(
            RequestPurpose::Settings(SettingsPurpose::GetPrivacyRules { key }),
            None,
        );
        session.settings.begin_privacy_load(key, load);
        let set = session.request(
            RequestPurpose::Settings(SettingsPurpose::SetPrivacyRules { key }),
            None,
        );
        session
            .settings
            .privacy
            .insert(key, PrivacyKeyState::Ready(who(PrivacyWho::Nobody)));
        session
            .settings
            .begin_privacy_save(key, who(PrivacyWho::Everybody), set);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"userPrivacySettingRules","@extra":"{}","rules":[{{"@type":"userPrivacySettingRuleAllowContacts"}}]}}"#,
                load.0
            ),
        );
        assert_eq!(
            session.settings.privacy[&key].detail(),
            Some(&who(PrivacyWho::Everybody))
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
        let PrivacyKeyState::Ready(detail) =
            &session.settings.privacy[&PrivacySettingKey::ShowStatus]
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
        assert_eq!(session.settings.privacy.len(), 1);
    }

    /// Slice S3: the read-date roundtrip — `get` stores the value and
    /// ends the stage; a failed `set` fails the stage and drops the
    /// optimistic value.
    #[test]
    fn read_date_get_and_set_failure() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(RequestPurpose::GetReadDatePrivacy, None);
        session.settings.read_date.start_get(extra);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"readDatePrivacySettings","@extra":"{}","show_read_date":false}}"#,
                extra.0,
            ),
        );
        assert_eq!(session.settings.read_date, ReadDate::Known(false));

        let extra = session.request(RequestPurpose::SetReadDatePrivacy, None);
        session.settings.read_date.start_set(extra, true); // optimistic set
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"BAD"}}"#,
                extra.0,
            ),
        );
        assert_eq!(session.settings.read_date, ReadDate::Failed { known: None });
        assert_eq!(session.settings.read_date.show(), None);
    }

    /// A get answered after a set went out does not overwrite the set
    /// value, and the set's own `ok` ends the stage with that value.
    #[test]
    fn read_date_get_answered_after_a_set_is_ignored() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let get = session.request(RequestPurpose::GetReadDatePrivacy, None);
        session.settings.read_date.start_get(get);
        let set = session.request(RequestPurpose::SetReadDatePrivacy, None);
        session.settings.read_date.start_set(set, false); // optimistic set
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"readDatePrivacySettings","@extra":"{}","show_read_date":true}}"#,
                get.0,
            ),
        );
        assert_eq!(session.settings.read_date.show(), Some(false));
        assert_eq!(
            session.settings.read_date,
            ReadDate::Setting {
                request: set,
                show: false
            }
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, set.0),
        );
        assert_eq!(session.settings.read_date, ReadDate::Known(false));
    }

    /// A page of a list that restarted since (or whose data was dropped)
    /// never extends the new one (05-132).
    #[test]
    fn a_late_blocked_page_is_matched_to_its_list() {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let old = session.request(
            RequestPurpose::Settings(SettingsPurpose::GetBlockedSenders { offset: 2 }),
            None,
        );
        session.settings.blocked_senders.begin_with(old);
        // The list restarted: its first page is in, and the fetch moved on.
        session.settings.blocked_senders = crate::state::fetch::Fetch::Idle;
        session.settings.blocked_senders.succeed(vec![61, 62]);
        session.settings.blocked_total = 2;
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"messageSenders","@extra":"{}","total_count":9,"senders":[{{"@type":"messageSenderUser","user_id":62}},{{"@type":"messageSenderUser","user_id":70}}]}}"#,
                old.0,
            ),
        );
        assert_eq!(session.settings.blocked_senders.data(), Some(&vec![61, 62]));
        assert_eq!(session.settings.blocked_total, 2);
        // A page of the live list appends without repeating an id.
        let live = session.request(
            RequestPurpose::Settings(SettingsPurpose::GetBlockedSenders { offset: 2 }),
            None,
        );
        session.settings.blocked_senders.begin_with(live);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"messageSenders","@extra":"{}","total_count":3,"senders":[{{"@type":"messageSenderUser","user_id":62}},{{"@type":"messageSenderUser","user_id":63}}]}}"#,
                live.0,
            ),
        );
        assert_eq!(
            session.settings.blocked_senders.data(),
            Some(&vec![61, 62, 63])
        );
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
        session.settings.blocked_senders.begin_with(extra);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"messageSenders","@extra":"{}","total_count":3,"senders":[{{"@type":"messageSenderUser","user_id":61}},{{"@type":"messageSenderUser","user_id":62}}]}}"#,
                extra.0,
            ),
        );
        assert_eq!(session.settings.blocked_senders.data(), Some(&vec![61, 62]));
        assert_eq!(session.settings.blocked_total, 3);
        assert!(!session.settings.blocked_senders.is_loading());

        let extra = session.request(
            RequestPurpose::Settings(SettingsPurpose::GetBlockedSenders { offset: 2 }),
            None,
        );
        session.settings.blocked_senders.begin_with(extra);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"messageSenders","@extra":"{}","total_count":3,"senders":[{{"@type":"messageSenderUser","user_id":63}}]}}"#,
                extra.0,
            ),
        );
        assert_eq!(
            session.settings.blocked_senders.data(),
            Some(&vec![61, 62, 63])
        );
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
