//! Chats TDLib shows in the main list as a promotion (`chatPosition.source`):
//! a proxy sponsor or a public service announcement. tdesktop paints a badge
//! where the date goes (`dialogs_layout.cpp`, `PaintRowTopRight`) and offers
//! "Hide this announcement" for announcements only (`addHidePromotion`).
#![deny(clippy::indexing_slicing)]

use crate::lang::{PackValue, keys, tr};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// Prefix of the server-provided label of an announcement type.
pub const PSA_LABEL_PREFIX: &str = "cloud_lng_badge_psa_";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Promotion {
    /// `chatSourceMtprotoProxy`: the proxy server sponsors the chat.
    MtprotoProxy,
    /// `chatSourcePublicServiceAnnouncement`; `kind` is its `type` (empty
    /// when TDLib sent none, which tdesktop cannot hide either).
    Psa { kind: String },
}

impl Promotion {
    /// The promotion a non-null `chatPosition.source` describes. An
    /// unknown source is shown like an announcement without a type.
    pub fn from_source(source: &Value) -> Self {
        match source.get("@type").and_then(Value::as_str) {
            Some("chatSourceMtprotoProxy") => Self::MtprotoProxy,
            _ => Self::Psa {
                kind: source
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            },
        }
    }

    /// The announcement type, when there is one.
    pub fn psa_kind(&self) -> Option<&str> {
        match self {
            Self::Psa { kind } if !kind.is_empty() => Some(kind),
            _ => None,
        }
    }

    /// tdesktop offers "Hide this announcement" only for a typed
    /// announcement; a proxy sponsor goes away with the proxy.
    pub fn can_hide(&self) -> bool {
        self.psa_kind().is_some()
    }
}

/// The server's badge text per announcement type, and which types were
/// already asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PsaLabels {
    cloud: HashMap<String, String>,
    requested: HashSet<String>,
}

impl PsaLabels {
    /// The row badge: "Proxy sponsor", the server's text for the type, or
    /// "PSA".
    pub fn label(&self, promotion: &Promotion) -> String {
        match promotion {
            Promotion::MtprotoProxy => tr(keys::PROXY_SPONSOR).to_string(),
            Promotion::Psa { kind } => self
                .cloud
                .get(kind)
                .cloned()
                .unwrap_or_else(|| tr(keys::BADGE_PSA_DEFAULT).to_string()),
        }
    }

    /// The language-pack keys still to fetch for `kinds`; each is asked
    /// for once.
    pub fn take_wanted<'a>(&mut self, kinds: impl IntoIterator<Item = &'a str>) -> Vec<String> {
        let mut wanted = Vec::new();
        for kind in kinds {
            if self.requested.insert(kind.to_string()) {
                wanted.push(format!("{PSA_LABEL_PREFIX}{kind}"));
            }
        }
        wanted
    }

    /// Keep the labels a `languagePackStrings` answer carries.
    pub fn store(&mut self, strings: Vec<(String, PackValue)>) {
        for (key, value) in strings {
            let (Some(kind), PackValue::Ordinary(text)) =
                (key.strip_prefix(PSA_LABEL_PREFIX), value)
            else {
                continue;
            };
            if !text.is_empty() {
                self.cloud.insert(kind.to_string(), text);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn source_types() {
        let proxy = Promotion::from_source(&json!({"@type": "chatSourceMtprotoProxy"}));
        assert_eq!(proxy, Promotion::MtprotoProxy);
        assert!(!proxy.can_hide());
        let psa = Promotion::from_source(&json!({
            "@type": "chatSourcePublicServiceAnnouncement",
            "type": "covid",
            "text": "Stay home",
        }));
        assert_eq!(psa.psa_kind(), Some("covid"));
        assert!(psa.can_hide());
        let untyped = Promotion::from_source(&json!({
            "@type": "chatSourcePublicServiceAnnouncement",
        }));
        assert!(!untyped.can_hide());
    }

    #[test]
    fn labels_prefer_the_servers_text() {
        let mut labels = PsaLabels::default();
        let psa = Promotion::Psa {
            kind: "covid".into(),
        };
        assert_eq!(labels.label(&psa), "PSA");
        assert_eq!(labels.label(&Promotion::MtprotoProxy), "Proxy sponsor");
        labels.store(vec![
            (
                "cloud_lng_badge_psa_covid".into(),
                PackValue::Ordinary("COVID-19 Info".into()),
            ),
            ("lng_other".into(), PackValue::Ordinary("x".into())),
            (
                "cloud_lng_badge_psa_empty".into(),
                PackValue::Ordinary(String::new()),
            ),
        ]);
        assert_eq!(labels.label(&psa), "COVID-19 Info");
        let empty = Promotion::Psa {
            kind: "empty".into(),
        };
        assert_eq!(labels.label(&empty), "PSA");
    }

    #[test]
    fn each_type_is_asked_for_once() {
        let mut labels = PsaLabels::default();
        assert_eq!(
            labels.take_wanted(["covid", "covid", "war"]),
            vec!["cloud_lng_badge_psa_covid", "cloud_lng_badge_psa_war"]
        );
        assert!(labels.take_wanted(["covid", "war"]).is_empty());
    }
}
