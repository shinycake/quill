use crate::ids::RequestId;
use serde_json::{Value, json};

/// Phase C2i: which call privacy setting a request targets —
/// `userPrivacySettingAllowCalls` (who can call me, schema 1.8.67
/// `:9006`) or `userPrivacySettingAllowPeerToPeerCalls` (P2P relay,
/// `:9009`). Both are standard privacy settings (Telegram X lists
/// both in its privacy screen).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallPrivacySetting {
    AllowCalls,
    PeerToPeer,
}

impl CallPrivacySetting {
    pub fn td_type(self) -> &'static str {
        match self {
            CallPrivacySetting::AllowCalls => "userPrivacySettingAllowCalls",
            CallPrivacySetting::PeerToPeer => "userPrivacySettingAllowPeerToPeerCalls",
        }
    }
}

/// Phase C2i: Everybody / Contacts / Nobody mapping for the two call
/// privacy settings. Telegram clients expose these as rule lists;
/// the three simple cases are `[AllowAll]`, `[AllowContacts]`,
/// `[RestrictAll]` (schema 1.8.67, `:8943`-`:8964`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyWho {
    Everybody,
    Contacts,
    Nobody,
}

impl PrivacyWho {
    /// The `userPrivacySettingRules` JSON for this choice.
    pub fn rules(self) -> Vec<Value> {
        let name = match self {
            PrivacyWho::Everybody => "userPrivacySettingRuleAllowAll",
            PrivacyWho::Contacts => "userPrivacySettingRuleAllowContacts",
            PrivacyWho::Nobody => "userPrivacySettingRuleRestrictAll",
        };
        vec![json!({"@type": name})]
    }

    /// Map server-returned rule constructor names back to the simple
    /// choice. Exception rules (`AllowUsers` / `RestrictUsers` / ...)
    /// are ignored: the list collapses to the first recognizable base
    /// rule in priority order (AllowAll > RestrictAll > AllowContacts);
    /// `None` only when no base rule is present (empty or fully custom
    /// lists the three-option UI cannot represent).
    pub fn from_rule_names(names: &[String]) -> Option<Self> {
        if names.iter().any(|n| n == "userPrivacySettingRuleAllowAll") {
            Some(PrivacyWho::Everybody)
        } else if names
            .iter()
            .any(|n| n == "userPrivacySettingRuleRestrictAll")
        {
            Some(PrivacyWho::Nobody)
        } else if names
            .iter()
            .any(|n| n == "userPrivacySettingRuleAllowContacts")
        {
            Some(PrivacyWho::Contacts)
        } else {
            None
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PrivacyWho::Everybody => "Everybody",
            PrivacyWho::Contacts => "My contacts",
            PrivacyWho::Nobody => "Nobody",
        }
    }
}

/// Phase C2i: `getUserPrivacySettingRules` (TDLib 1.8.67,
/// `schema/td_api.tl:15620`): "Returns the current privacy settings".
/// `getUserPrivacySettingRules setting:UserPrivacySetting =
/// UserPrivacySettingRules;`
pub fn get_user_privacy_setting_rules(extra: RequestId, setting: CallPrivacySetting) -> String {
    json!({
        "@type": "getUserPrivacySettingRules",
        "@extra": extra.as_extra(),
        "setting": {"@type": setting.td_type()},
    })
    .to_string()
}

/// Phase C2i: `setUserPrivacySettingRules` (TDLib 1.8.67,
/// `schema/td_api.tl:15617`): "Changes user privacy settings".
/// `setUserPrivacySettingRules setting:UserPrivacySetting
/// rules:userPrivacySettingRules = Ok;`
pub fn set_user_privacy_setting_rules(
    extra: RequestId,
    setting: CallPrivacySetting,
    who: PrivacyWho,
) -> String {
    json!({
        "@type": "setUserPrivacySettingRules",
        "@extra": extra.as_extra(),
        "setting": {"@type": setting.td_type()},
        "rules": {"@type": "userPrivacySettingRules", "rules": who.rules()},
    })
    .to_string()
}
