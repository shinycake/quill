use crate::ids::RequestId;
use crate::telegram::requests::*;

#[test]
fn search_public_chat_shape_matches_1_8_67() {
    // `searchPublicChat username:string = Chat` (schema 1.8.67, line
    // 11603); singular lookup returning the chat itself.
    let json = search_public_chat(RequestId(25), "gif");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "searchPublicChat");
    assert_eq!(v["@extra"], "25");
    assert_eq!(v["username"], "gif");
    let schema = include_str!("../../../schema/td_api.tl");
    let line = schema
        .lines()
        .find(|l| l.starts_with("searchPublicChat "))
        .expect("searchPublicChat in schema");
    assert_eq!(line, "searchPublicChat username:string = Chat;");
}

#[test]
fn deep_link_type_requests_match_the_schema() {
    let schema = include_str!("../../../schema/td_api.tl");
    let cases = [
        (
            get_internal_link_type(RequestId(1), "https://t.me/x"),
            "getInternalLinkType",
            "link",
            "https://t.me/x",
            "getInternalLinkType link:string = InternalLinkType;",
        ),
        (
            get_message_link_info(RequestId(2), "https://t.me/c/1/2"),
            "getMessageLinkInfo",
            "url",
            "https://t.me/c/1/2",
            "getMessageLinkInfo url:string = MessageLinkInfo;",
        ),
        (
            search_sticker_set_by_name(RequestId(3), "Cats"),
            "searchStickerSet",
            "name",
            "Cats",
            "searchStickerSet name:string ignore_cache:Bool = StickerSet;",
        ),
        (
            search_user_by_phone_number(RequestId(4), "15550001"),
            "searchUserByPhoneNumber",
            "phone_number",
            "15550001",
            "searchUserByPhoneNumber phone_number:string only_local:Bool = User;",
        ),
    ];
    for (json, ty, field, value, line) in cases {
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], ty);
        assert_eq!(v[field], value);
        assert!(schema.lines().any(|l| l == line), "{line}");
    }
}
