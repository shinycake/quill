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
