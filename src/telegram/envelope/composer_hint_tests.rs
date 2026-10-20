//! What the field hints depend on: whether the viewer posts as the group
//! and an inline bot's own placeholder.

use super::chat_members::parse_anonymous_status;
use super::users::parse_user;
use serde_json::{Value, json};

fn administrator(is_anonymous: bool) -> Value {
    json!({
        "@type": "chatMemberStatusAdministrator",
        "can_be_edited": false,
        "rights": {"@type": "chatAdministratorRights", "is_anonymous": is_anonymous},
    })
}

fn anonymous(status: Value) -> Option<bool> {
    Some(parse_anonymous_status(Some(&status)))
}

#[test]
fn an_administrator_is_anonymous_by_their_rights() {
    assert_eq!(anonymous(administrator(true)), Some(true));
    assert_eq!(anonymous(administrator(false)), Some(false));
}

#[test]
fn the_owner_is_anonymous_by_their_status() {
    let creator = |is_anonymous| json!({"@type": "chatMemberStatusCreator", "is_anonymous": is_anonymous, "is_member": true});
    assert_eq!(anonymous(creator(true)), Some(true));
    assert_eq!(anonymous(creator(false)), Some(false));
}

#[test]
fn a_member_is_never_anonymous() {
    assert_eq!(
        anonymous(json!({"@type": "chatMemberStatusMember", "is_anonymous": true})),
        Some(false)
    );
}

#[test]
fn a_bot_carries_its_inline_placeholder() {
    let bot = |placeholder: &str| {
        json!({
            "@type": "user",
            "id": 61,
            "first_name": "GIF",
            "type": {
                "@type": "userTypeBot",
                "is_inline": true,
                "inline_query_placeholder": placeholder,
            },
        })
    };
    let parsed = parse_user(&bot("Search GIFs"));
    assert_eq!(
        parsed.map(|u| u.inline_query_placeholder).as_deref(),
        Some("Search GIFs")
    );
    let regular = json!({"@type": "user", "id": 62, "first_name": "Ada", "type": {"@type": "userTypeRegular"}});
    assert_eq!(
        parse_user(&regular).map(|u| u.inline_query_placeholder),
        Some(String::new())
    );
}
