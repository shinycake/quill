use quill::ids::RequestId;
use quill::telegram::requests::{search_messages, search_messages_chat_type_filter_json};
use serde_json::Value;

#[test]
fn community_chat_type_filter_none_is_null() {
    // No community selected: the historical all-chat-types behavior (null
    // filter slot in `searchMessages`).
    assert_eq!(search_messages_chat_type_filter_json(None), Value::Null);
}

#[test]
fn community_chat_type_filter_builds_community_constructor() {
    let filter = search_messages_chat_type_filter_json(Some(42));
    assert_eq!(filter["@type"], "searchMessagesChatTypeFilterCommunity");
    assert_eq!(filter["community_id"], 42);
}

#[test]
fn search_messages_carries_community_filter_payload() {
    // The selected community id round-trips from the chip selection into the
    // `searchMessages` `chat_type_filter` slot.
    let filter = search_messages_chat_type_filter_json(Some(42));
    let json = search_messages(RequestId(7), "hello", 20, filter);
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "searchMessages");
    assert_eq!(
        v["chat_type_filter"]["@type"],
        "searchMessagesChatTypeFilterCommunity"
    );
    assert_eq!(v["chat_type_filter"]["community_id"], 42);

    // `None` keeps the historical all-chat-types behavior.
    let json = search_messages(
        RequestId(8),
        "hello",
        20,
        search_messages_chat_type_filter_json(None),
    );
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["chat_type_filter"], Value::Null);
}
