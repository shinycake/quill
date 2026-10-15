//! Where Quill links out to Fragment, from the app config as tdesktop
//! reads it (`main/main_app_config_values.cpp` `FragmentLink`,
//! `info/profile/info_profile_phone_menu.cpp` `IsCollectiblePhone`).
//!
//! The Fragment address is the first entry of `whitelisted_domains` that
//! contains "fragment"; without one, tdesktop shows no Fragment link at
//! all. Anonymous numbers start with one of `fragment_prefixes` (default
//! "888").

use serde_json::Value;

/// `whitelisted_domains` and `fragment_prefixes` from the app config.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FragmentConfig {
    /// The Fragment address as the config gives it, with a scheme.
    pub link: Option<String>,
    /// `fragment_prefixes`; empty until the config says otherwise (the
    /// default "888" applies).
    pub prefixes: Vec<String>,
}

fn members<'a>(config: &'a Value, key: &str) -> Option<&'a Value> {
    config
        .get("members")
        .and_then(Value::as_array)?
        .iter()
        .find(|m| m.get("key").and_then(Value::as_str) == Some(key))
        .and_then(|m| m.get("value"))
}

/// The strings of a `jsonValueArray` member.
fn string_list(config: &Value, key: &str) -> Vec<String> {
    members(config, key)
        .and_then(|v| v.get("values"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| v.get("value")?.as_str().map(str::to_string))
        .collect()
}

/// The Fragment settings of a `getApplicationConfig` answer.
pub fn fragment_from_config(config: &Value) -> FragmentConfig {
    let link = string_list(config, "whitelisted_domains")
        .into_iter()
        .find(|domain| domain.contains("fragment"))
        .map(|domain| {
            if domain.contains("://") {
                domain
            } else {
                format!("https://{domain}")
            }
        });
    FragmentConfig {
        link,
        prefixes: string_list(config, "fragment_prefixes"),
    }
}

impl FragmentConfig {
    /// Whether this phone number was bought on Fragment
    /// (`IsCollectiblePhone`).
    pub fn is_collectible_phone(&self, phone: &str) -> bool {
        let digits: String = phone.chars().filter(char::is_ascii_digit).collect();
        if self.prefixes.is_empty() {
            return digits.starts_with("888");
        }
        self.prefixes.iter().any(|prefix| {
            let prefix: String = prefix.chars().filter(char::is_ascii_digit).collect();
            !prefix.is_empty() && digits.starts_with(&prefix)
        })
    }

    /// `<fragment>/username/<name>`, where a taken username can be bought.
    pub fn username_url(&self, username: &str) -> Option<String> {
        let link = self.link.as_deref()?;
        Some(format!(
            "{}/username/{username}",
            link.trim_end_matches('/')
        ))
    }

    /// `<fragment>/gift/<slug>?collection=my`, where a gift that left
    /// Telegram is managed (`ShowActionLocked`).
    pub fn gift_url(slug: &str) -> String {
        format!("https://fragment.com/gift/{slug}?collection=my")
    }
}

#[cfg(test)]
mod tests {
    use super::{FragmentConfig, fragment_from_config};
    use serde_json::json;

    fn config() -> serde_json::Value {
        json!({"@type": "jsonValueObject", "members": [
            {"key": "whitelisted_domains", "value": {"@type": "jsonValueArray", "values": [
                {"@type": "jsonValueString", "value": "telegram.org"},
                {"@type": "jsonValueString", "value": "fragment.com"},
            ]}},
            {"key": "fragment_prefixes", "value": {"@type": "jsonValueArray", "values": [
                {"@type": "jsonValueString", "value": "888"},
                {"@type": "jsonValueString", "value": "999"},
            ]}},
        ]})
    }

    #[test]
    fn the_first_fragment_domain_is_the_link() {
        let fragment = fragment_from_config(&config());
        assert_eq!(fragment.link.as_deref(), Some("https://fragment.com"));
        assert_eq!(
            fragment.username_url("durov").as_deref(),
            Some("https://fragment.com/username/durov")
        );
        assert_eq!(fragment_from_config(&json!({})).link, None);
        assert_eq!(FragmentConfig::default().username_url("x"), None);
    }

    #[test]
    fn numbers_follow_the_prefixes_and_default_to_888() {
        let fragment = fragment_from_config(&config());
        assert!(fragment.is_collectible_phone("+999 1234"));
        assert!(!fragment.is_collectible_phone("972502002287"));
        assert!(FragmentConfig::default().is_collectible_phone("88812345678"));
        assert!(!FragmentConfig::default().is_collectible_phone(""));
    }
}
