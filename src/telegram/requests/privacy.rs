use serde_json::{Value, json};

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
