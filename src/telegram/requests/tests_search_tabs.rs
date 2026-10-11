use crate::ids::RequestId;
use crate::search_filters::{SearchChatType, SearchMediaKind};
use crate::telegram::requests::*;

fn parse(json: &str) -> serde_json::Value {
    serde_json::from_str(json).unwrap()
}

#[test]
fn a_later_page_carries_its_offset_and_the_filters() {
    let filters = SearchMessagesFilters {
        media: SearchMediaKind::Photos,
        chat_type: SearchChatType::Channels,
        ..Default::default()
    };
    let v = parse(&search_messages_page(
        RequestId(4),
        "cats",
        "next-1",
        20,
        &filters,
    ));
    assert_eq!(v["@type"], "searchMessages");
    assert_eq!(v["offset"], "next-1");
    assert_eq!(v["query"], "cats");
    assert_eq!(v["filter"]["@type"], "searchMessagesFilterPhoto");
    assert_eq!(
        v["chat_type_filter"]["@type"],
        "searchMessagesChatTypeFilterChannel"
    );
}

#[test]
fn the_first_page_keeps_an_empty_offset() {
    let v = parse(&search_messages_filtered(
        RequestId(4),
        "",
        20,
        &SearchMessagesFilters {
            media: SearchMediaKind::Videos,
            ..Default::default()
        },
    ));
    assert_eq!(v["offset"], "");
    assert_eq!(v["query"], "");
    assert_eq!(v["filter"]["@type"], "searchMessagesFilterVideo");
    assert!(v["chat_type_filter"].is_null());
}

#[test]
fn tab_requests_match_the_schema() {
    let schema = include_str!("../../../schema/td_api.tl");
    let has = |line: &str| schema.lines().any(|l| l == line);
    assert!(has("getRecommendedChats = Chats;"));
    assert!(has(
        "getPublicPostSearchLimits query:string = PublicPostSearchLimits;"
    ));
    let v = parse(&get_recommended_chats(RequestId(1)));
    assert_eq!(v["@type"], "getRecommendedChats");
    let v = parse(&get_public_post_search_limits(RequestId(2), "rust"));
    assert_eq!(v["@type"], "getPublicPostSearchLimits");
    assert_eq!(v["query"], "rust");
}
