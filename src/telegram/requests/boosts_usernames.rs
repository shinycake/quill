//! Chat boost list/link and supergroup username builders (batch
//! "admin links, boosts, usernames").
use crate::ids::RequestId;
use serde_json::json;

/// `getChatBoosts` (schema/td_api.tl:14329). `offset` is the previous
/// page's `next_offset` (empty for the first page).
pub fn get_chat_boosts(
    extra: RequestId,
    chat_id: i64,
    only_gift_codes: bool,
    offset: &str,
    limit: i32,
) -> String {
    json!({
        "@type": "getChatBoosts",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "only_gift_codes": only_gift_codes,
        "offset": offset,
        "limit": limit,
    })
    .to_string()
}

/// `getChatBoostLink` (schema/td_api.tl:14319).
pub fn get_chat_boost_link(extra: RequestId, chat_id: i64) -> String {
    json!({
        "@type": "getChatBoostLink",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
    })
    .to_string()
}

/// `toggleSupergroupUsernameIsActive` (schema/td_api.tl:15550).
pub fn toggle_supergroup_username_is_active(
    extra: RequestId,
    supergroup_id: i64,
    username: &str,
    is_active: bool,
) -> String {
    json!({
        "@type": "toggleSupergroupUsernameIsActive",
        "@extra": extra.as_extra(),
        "supergroup_id": supergroup_id,
        "username": username,
        "is_active": is_active,
    })
    .to_string()
}

/// `reorderSupergroupActiveUsernames` (schema/td_api.tl:15558): the full
/// new order of the active usernames.
pub fn reorder_supergroup_active_usernames(
    extra: RequestId,
    supergroup_id: i64,
    usernames: &[String],
) -> String {
    json!({
        "@type": "reorderSupergroupActiveUsernames",
        "@extra": extra.as_extra(),
        "supergroup_id": supergroup_id,
        "usernames": usernames,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn parse(json: &str) -> Value {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn boosts_page_shape() {
        let v = parse(&get_chat_boosts(RequestId(1), -100, true, "abc", 25));
        assert_eq!(v["@type"], "getChatBoosts");
        assert_eq!(v["chat_id"], -100);
        assert_eq!(v["only_gift_codes"], true);
        assert_eq!(v["offset"], "abc");
        assert_eq!(v["limit"], 25);
    }

    #[test]
    fn boost_link_shape() {
        let v = parse(&get_chat_boost_link(RequestId(2), -100));
        assert_eq!(v["@type"], "getChatBoostLink");
        assert_eq!(v["chat_id"], -100);
    }

    #[test]
    fn supergroup_username_shapes() {
        let t = parse(&toggle_supergroup_username_is_active(
            RequestId(3),
            55,
            "rustaceans",
            false,
        ));
        assert_eq!(t["@type"], "toggleSupergroupUsernameIsActive");
        assert_eq!(t["supergroup_id"], 55);
        assert_eq!(t["username"], "rustaceans");
        assert_eq!(t["is_active"], false);
        let r = parse(&reorder_supergroup_active_usernames(
            RequestId(4),
            55,
            &["b".to_string(), "a".to_string()],
        ));
        assert_eq!(r["@type"], "reorderSupergroupActiveUsernames");
        assert_eq!(r["usernames"], serde_json::json!(["b", "a"]));
    }
}

/// `getChatBoostFeatures` (schema/td_api.tl:14305): the level table for
/// channels or groups.
pub fn get_chat_boost_features(extra: RequestId, is_channel: bool) -> String {
    json!({
        "@type": "getChatBoostFeatures",
        "@extra": extra.as_extra(),
        "is_channel": is_channel,
    })
    .to_string()
}

/// `setSupergroupUnrestrictBoostCount` (schema/td_api.tl:15571): boosts a
/// user needs to ignore slow mode and permission restrictions; 0-8, 0
/// removes the setting.
pub fn set_supergroup_unrestrict_boost_count(
    extra: RequestId,
    supergroup_id: i64,
    unrestrict_boost_count: i32,
) -> String {
    json!({
        "@type": "setSupergroupUnrestrictBoostCount",
        "@extra": extra.as_extra(),
        "supergroup_id": supergroup_id,
        "unrestrict_boost_count": unrestrict_boost_count,
    })
    .to_string()
}

#[cfg(test)]
mod boost_settings_tests {
    use super::{get_chat_boost_features, set_supergroup_unrestrict_boost_count};
    use crate::ids::RequestId;
    use serde_json::Value;

    #[test]
    fn boost_features_request() {
        let v: Value = serde_json::from_str(&get_chat_boost_features(RequestId(4), true)).unwrap();
        assert_eq!(v["@type"], "getChatBoostFeatures");
        assert_eq!(v["is_channel"], true);
    }

    #[test]
    fn unrestrict_boost_count_request() {
        let v: Value =
            serde_json::from_str(&set_supergroup_unrestrict_boost_count(RequestId(5), 77, 3))
                .unwrap();
        assert_eq!(v["@type"], "setSupergroupUnrestrictBoostCount");
        assert_eq!(v["supergroup_id"], 77);
        assert_eq!(v["unrestrict_boost_count"], 3);
    }
}
