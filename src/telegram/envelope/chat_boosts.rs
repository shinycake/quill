//! `foundChatBoosts`, `chatBoostLink` and supergroup `usernames` parsing.
use super::*;
use serde_json::Value;

/// Where a boost came from (`ChatBoostSource`, schema/td_api.tl:7244+).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedBoostSource {
    /// A Telegram Premium subscriber boosting directly.
    Premium { user_id: i64 },
    /// A gift code the user redeemed.
    GiftCode { user_id: i64 },
    /// A giveaway prize; `is_unclaimed` boosts have no recipient yet.
    Giveaway { user_id: i64, is_unclaimed: bool },
}

impl ParsedBoostSource {
    /// The booster, if known (0 for unclaimed giveaway prizes).
    pub fn user_id(&self) -> i64 {
        match self {
            Self::Premium { user_id }
            | Self::GiftCode { user_id }
            | Self::Giveaway { user_id, .. } => *user_id,
        }
    }

    /// Gift codes and giveaways show on the "Gifts" tab.
    pub fn is_gift(&self) -> bool {
        !matches!(self, Self::Premium { .. })
    }
}

/// `chatBoost` (schema/td_api.tl:7286).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedChatBoost {
    pub id: String,
    pub count: i32,
    pub source: ParsedBoostSource,
    pub start_date: i32,
    pub expiration_date: i32,
}

pub(crate) fn parse_chat_boost(value: Option<&Value>) -> Option<ParsedChatBoost> {
    let value = value?;
    let source = value.get("source")?;
    let user_id = int53(source.get("user_id")).unwrap_or(0);
    let source = match source.get("@type").and_then(Value::as_str)? {
        "chatBoostSourcePremium" => ParsedBoostSource::Premium { user_id },
        "chatBoostSourceGiftCode" => ParsedBoostSource::GiftCode { user_id },
        "chatBoostSourceGiveaway" => ParsedBoostSource::Giveaway {
            user_id,
            is_unclaimed: source
                .get("is_unclaimed")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
        _ => return None,
    };
    Some(ParsedChatBoost {
        id: value.get("id").and_then(Value::as_str)?.to_owned(),
        count: int53(value.get("count")).unwrap_or(1).sat_i32().max(1),
        source,
        start_date: int53(value.get("start_date")).unwrap_or(0).sat_i32(),
        expiration_date: int53(value.get("expiration_date")).unwrap_or(0).sat_i32(),
    })
}

/// A supergroup's or channel's username lists (`usernames`,
/// schema/td_api.tl:2681). The editable one cannot be deactivated;
/// `collectible` ones are Fragment usernames.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SupergroupUsernames {
    pub active: Vec<String>,
    pub disabled: Vec<String>,
    pub editable: String,
    pub collectible: Vec<String>,
}

impl SupergroupUsernames {
    /// tdesktop shows the "Link order" list only when there is more than
    /// the single editable username to manage.
    pub fn is_manageable(&self) -> bool {
        !self.collectible.is_empty() || self.active.len() + self.disabled.len() > 1
    }
}

pub(crate) fn parse_supergroup_usernames(value: Option<&Value>) -> SupergroupUsernames {
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return SupergroupUsernames::default();
    };
    let list = |name: &str| -> Vec<String> {
        value
            .get(name)
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    SupergroupUsernames {
        active: list("active_usernames"),
        disabled: list("disabled_usernames"),
        editable: value
            .get("editable_username")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        collectible: list("collectible_usernames"),
    }
}

#[cfg(test)]
mod tests {
    use super::{ParsedBoostSource, SupergroupUsernames};
    use crate::telegram::envelope::GroupsPayload;
    use crate::telegram::envelope::{EnvelopePayload, parse_envelope};

    #[test]
    fn found_chat_boosts_parse() {
        let json = r#"{"@type":"foundChatBoosts","total_count":3,"boosts":[{"@type":"chatBoost","id":"b1","count":2,"source":{"@type":"chatBoostSourcePremium","user_id":9},"start_date":100,"expiration_date":200},{"@type":"chatBoost","id":"b2","count":1,"source":{"@type":"chatBoostSourceGiveaway","user_id":0,"gift_code":"","star_count":0,"giveaway_message_id":4,"is_unclaimed":true},"start_date":100,"expiration_date":300}],"next_offset":"nx"}"#;
        match parse_envelope(json).unwrap().payload {
            EnvelopePayload::Groups(GroupsPayload::FoundChatBoosts {
                total_count,
                boosts,
                next_offset,
            }) => {
                assert_eq!(total_count, 3);
                assert_eq!(next_offset, "nx");
                assert_eq!(boosts.len(), 2);
                assert_eq!(boosts[0].count, 2);
                assert_eq!(boosts[0].source.user_id(), 9);
                assert!(!boosts[0].source.is_gift());
                assert!(matches!(
                    boosts[1].source,
                    ParsedBoostSource::Giveaway {
                        is_unclaimed: true,
                        ..
                    }
                ));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn chat_boost_link_parses() {
        let json =
            r#"{"@type":"chatBoostLink","link":"https://t.me/boost/rustaceans","is_public":true}"#;
        match parse_envelope(json).unwrap().payload {
            EnvelopePayload::Groups(GroupsPayload::ChatBoostLink { link, is_public }) => {
                assert_eq!(link, "https://t.me/boost/rustaceans");
                assert!(is_public);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn update_supergroup_keeps_username_lists() {
        let json = r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":18,"usernames":{"@type":"usernames","active_usernames":["a","b"],"disabled_usernames":["c"],"editable_username":"a","collectible_usernames":["b"]}}}"#;
        match parse_envelope(json).unwrap().payload {
            EnvelopePayload::Groups(GroupsPayload::UpdateSupergroup { usernames, .. }) => {
                assert_eq!(usernames.active, vec!["a", "b"]);
                assert_eq!(usernames.disabled, vec!["c"]);
                assert_eq!(usernames.editable, "a");
                assert_eq!(usernames.collectible, vec!["b"]);
                assert!(usernames.is_manageable());
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn single_editable_username_is_not_manageable() {
        let one = SupergroupUsernames {
            active: vec!["a".into()],
            editable: "a".into(),
            ..Default::default()
        };
        assert!(!one.is_manageable());
        assert!(!SupergroupUsernames::default().is_manageable());
    }
}
