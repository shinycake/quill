//! Find in history: "From:" member search, "N of M" paging, the calendar
//! box and jump to date (recorded TDLib JSON in, request JSON out).
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId, MessageId};
use crate::platform::MemorySecretStore;
use crate::search_filters::{SearchMediaKind, day_number};
use crate::state::{ChatSearchJump, Session};
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::MessageSender;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const GROUP: i64 = 14;

struct Fixture {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    sink: Arc<dyn DiagnosticSink>,
    seq: AtomicU64,
    dir: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let mem = Arc::new(MemorySink::new());
        let sink: Arc<dyn DiagnosticSink> = mem;
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), sink.clone());
        let driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let mut fx = Self {
            driver,
            recorder,
            sink,
            seq: AtomicU64::new(0),
            dir,
        };
        fx.feed(r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#);
        fx.feed(&format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{GROUP},"title":"Book club","type":{{"@type":"chatTypeSupergroup","supergroup_id":{GROUP},"is_channel":false}},"unread_count":0}}}}"#
        ));
        fx.feed(&format!(
            r#"{{"@type":"updateChatPosition","chat_id":{GROUP},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"9","is_pinned":false}}}}"#
        ));
        fx.feed(&format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{},"chat_id":{GROUP},"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"latest","entities":[]}}}}}}}}"#,
            900i64 << 20
        ));
        fx.driver.select_chat(ChatId(GROUP)).unwrap();
        fx
    }

    fn feed(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    /// The newest request of `type_name` the driver sent.
    fn last_request(&self, type_name: &str) -> Value {
        let needle = format!("\"@type\":\"{type_name}\"");
        let json = self
            .recorder
            .snapshot()
            .into_iter()
            .rev()
            .find(|j| j.contains(&needle) && !j.contains("searchMessagesFilterPinned"))
            .unwrap_or_else(|| panic!("no {type_name} request"));
        serde_json::from_str(&json).unwrap()
    }

    fn count(&self, type_name: &str) -> usize {
        let needle = format!("\"@type\":\"{type_name}\"");
        self.recorder
            .snapshot()
            .iter()
            .filter(|j| j.contains(&needle) && !j.contains("searchMessagesFilterPinned"))
            .count()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn text_message(id: i64, sender: i64, text: &str) -> String {
    format!(
        r#"{{"id":{id},"chat_id":{GROUP},"date":1790000000,"sender_id":{{"@type":"messageSenderUser","user_id":{sender}}},"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}}"#
    )
}

fn extra_of(request: &Value) -> String {
    request["@extra"].as_str().unwrap().to_string()
}

#[test]
fn from_member_search_filters_by_sender_and_pages_with_total() {
    let mut fx = Fixture::new();
    assert!(
        fx.driver.session.chat_search_can_pick_sender() || {
            assert!(fx.driver.open_chat_search().unwrap());
            fx.driver.session.chat_search_can_pick_sender()
        }
    );
    assert!(fx.driver.open_chat_search().unwrap());

    // The picker lists the group's members.
    fx.driver.open_chat_search_from_picker().unwrap();
    let members = fx.last_request("searchChatMembers");
    assert_eq!(members["chat_id"], GROUP);
    assert_eq!(members["query"], "");
    fx.feed(&format!(
        r#"{{"@type":"chatMembers","@extra":"{}","total_count":2,"members":[{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":5}},"status":{{"@type":"chatMemberStatusMember"}}}},{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":6}},"status":{{"@type":"chatMemberStatusMember"}}}}]}}"#,
        extra_of(&members)
    ));
    let picker = fx
        .driver
        .session
        .search
        .chat_search
        .from_picker
        .clone()
        .unwrap();
    assert_eq!(
        picker.members,
        vec![
            MessageSender::User { user_id: 5 },
            MessageSender::User { user_id: 6 }
        ]
    );

    // A late answer for an older picker query is dropped.
    fx.driver.search_from_members("zed").unwrap();
    fx.feed(&format!(
        r#"{{"@type":"chatMembers","@extra":"{}","total_count":1,"members":[{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":9}},"status":{{"@type":"chatMemberStatusMember"}}}}]}}"#,
        extra_of(&members)
    ));
    assert_eq!(
        fx.driver
            .session
            .search
            .chat_search
            .from_picker
            .as_ref()
            .unwrap()
            .members
            .len(),
        2
    );

    // Choosing a member searches that sender's messages, no text needed.
    fx.driver
        .set_chat_search_sender(Some(MessageSender::User { user_id: 5 }))
        .unwrap();
    assert!(fx.driver.session.search.chat_search.from_picker.is_none());
    let search = fx.last_request("searchChatMessages");
    assert_eq!(
        search["sender_id"],
        serde_json::json!({"@type":"messageSenderUser","user_id":5})
    );
    assert_eq!(search["query"], "");
    assert_eq!(search["from_message_id"], 0);
    fx.feed(&format!(
        r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":120,"next_from_message_id":{},"messages":[{},{}]}}"#,
        extra_of(&search),
        40i64 << 20,
        text_message(50 << 20, 5, "newest from five"),
        text_message(40 << 20, 5, "older from five"),
    ));
    let state = &fx.driver.session.search.chat_search;
    assert_eq!(state.hits.len(), 2);
    assert_eq!(state.position_label(), "1 of 120");

    // Walking older reaches the page end and fetches the next page with
    // the same sender and the cursor.
    fx.driver.chat_search_older().unwrap();
    let more = fx.last_request("searchChatMessages");
    assert_eq!(more["from_message_id"], 40i64 << 20);
    assert_eq!(more["sender_id"]["user_id"], 5);
    assert!(fx.driver.session.search.chat_search.loading_more);
    fx.feed(&format!(
        r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":120,"next_from_message_id":{},"messages":[{}]}}"#,
        extra_of(&more),
        30i64 << 20,
        text_message(30 << 20, 5, "third from five"),
    ));
    let state = &fx.driver.session.search.chat_search;
    assert_eq!(state.hits.len(), 3);
    assert!(!state.loading_more);
    // Appending must not steal the selection or restart the jump.
    assert_eq!(state.position_label(), "2 of 120");
    assert_eq!(fx.count("searchChatMessages"), 2);

    // Clearing the member (no text, no media tab) ends the search.
    fx.driver.set_chat_search_sender(None).unwrap();
    assert!(fx.driver.session.search.chat_search.hits.is_empty());
    assert!(!fx.driver.session.search.chat_search.has_criteria());
}

#[test]
fn private_chats_and_channels_have_no_from_picker() {
    let mut fx = Fixture::new();
    fx.feed(
        r#"{"@type":"updateNewChat","chat":{"id":8,"title":"News","type":{"@type":"chatTypeSupergroup","supergroup_id":8,"is_channel":true},"unread_count":0}}"#,
    );
    fx.feed(
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
    );
    for id in [8, 7] {
        fx.driver.session.open_chat = Some(ChatId(id));
        assert!(fx.driver.session.open_chat_search());
        assert!(!fx.driver.session.chat_search_can_pick_sender(), "{id}");
        assert!(fx.driver.open_chat_search_from_picker().is_err());
        fx.driver.session.close_chat_search();
    }
}

#[test]
fn media_tab_filters_in_chat_search() {
    let mut fx = Fixture::new();
    assert!(fx.driver.open_chat_search().unwrap());
    fx.driver
        .set_chat_search_media(SearchMediaKind::Links)
        .unwrap();
    let search = fx.last_request("searchChatMessages");
    assert_eq!(search["filter"]["@type"], "searchMessagesFilterUrl");
    assert_eq!(search["sender_id"], Value::Null);
    fx.driver
        .set_chat_search_media(SearchMediaKind::All)
        .unwrap();
    assert!(!fx.driver.session.search.chat_search.has_criteria());
}

#[test]
fn calendar_highlights_media_days_and_jumps_to_their_first_message() {
    let mut fx = Fixture::new();
    assert!(fx.driver.open_chat_search().unwrap());
    fx.driver
        .set_chat_search_media(SearchMediaKind::Media)
        .unwrap();
    fx.driver.open_history_calendar().unwrap();
    let calendar = fx.last_request("getChatMessageCalendar");
    assert_eq!(calendar["chat_id"], GROUP);
    assert_eq!(calendar["topic_id"], Value::Null);
    assert_eq!(
        calendar["filter"]["@type"],
        "searchMessagesFilterPhotoAndVideo"
    );
    assert_eq!(calendar["from_message_id"], 0);
    let day = |id: i64, date: i64, count: i64| {
        format!(
            r#"{{"@type":"messageCalendarDay","total_count":{count},"message":{{"id":{id},"chat_id":{GROUP},"date":{date},"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"x","entities":[]}}}}}}}}"#
        )
    };
    fx.feed(&format!(
        r#"{{"@type":"messageCalendar","@extra":"{}","total_count":5,"days":[{},{}]}}"#,
        extra_of(&calendar),
        day(300 << 20, 1_790_000_000, 3),
        day(200 << 20, 1_789_000_000, 2),
    ));
    let cal = fx.driver.session.search.history_calendar.as_ref().unwrap();
    assert_eq!(cal.days.len(), 2);
    assert!(!cal.loading);
    let picked = crate::search_filters::local_day_number(1_789_000_000);

    // Paging back for an older month continues from the oldest message.
    let older = crate::search_filters::YearMonth::of(1_700_000_000);
    fx.driver.show_calendar_month(older).unwrap();
    let page = fx.last_request("getChatMessageCalendar");
    assert_eq!(page["from_message_id"], 200i64 << 20);

    // A highlighted day jumps straight to its first message.
    fx.driver.jump_to_date(picked).unwrap();
    assert!(fx.driver.session.search.history_calendar.is_none());
    assert_eq!(fx.count("getChatMessageByDate"), 0);
    let around = fx.last_request("getChatHistory");
    assert_eq!(around["from_message_id"], 200i64 << 20);
}

#[test]
fn unfiltered_calendar_sends_no_calendar_request() {
    let mut fx = Fixture::new();
    fx.driver.open_history_calendar().unwrap();
    // TDLib rejects the empty filter, so nothing is requested.
    assert_eq!(fx.count("getChatMessageCalendar"), 0);
    assert!(
        fx.driver
            .session
            .search
            .history_calendar
            .as_ref()
            .unwrap()
            .exhausted
    );
}

#[test]
fn jump_to_date_lands_on_the_first_message_of_the_day() {
    let mut fx = Fixture::new();
    fx.driver.open_history_calendar().unwrap();
    let day = day_number(2026, 9, 3);
    fx.driver.jump_to_date(day).unwrap();
    let by_date = fx.last_request("getChatMessageByDate");
    assert_eq!(by_date["chat_id"], GROUP);
    assert_eq!(
        by_date["date"].as_i64().unwrap(),
        i64::from(crate::search_filters::before_day_date(day))
    );

    // TDLib answers with the last message before the day...
    fx.feed(&format!(
        r#"{{"@type":"message","@extra":"{}","id":{},"chat_id":{GROUP},"date":1788000000,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"late on the 2nd","entities":[]}}}}}}"#,
        extra_of(&by_date),
        60i64 << 20,
    ));
    let around = fx.last_request("getChatHistory");
    assert_eq!(around["from_message_id"], 60i64 << 20);
    let messages = [60i64, 61, 62]
        .iter()
        .rev()
        .map(|id| text_message(id << 20, 5, "m"))
        .collect::<Vec<_>>()
        .join(",");
    fx.feed(&format!(
        r#"{{"@type":"messages","@extra":"{}","total_count":3,"messages":[{messages}]}}"#,
        extra_of(&around)
    ));
    // ...and the jump settles on the next one: the day's first message.
    assert_eq!(
        fx.driver.session.search.chat_search.jump,
        ChatSearchJump::Ready {
            message_id: MessageId(61 << 20)
        }
    );
}

#[test]
fn jump_to_a_date_before_the_chat_starts_goes_to_the_oldest_message() {
    let mut fx = Fixture::new();
    fx.driver.open_history_calendar().unwrap();
    fx.driver.jump_to_date(day_number(2013, 9, 1)).unwrap();
    let by_date = fx.last_request("getChatMessageByDate");
    fx.feed(&format!(
        r#"{{"@type":"error","@extra":"{}","code":404,"message":"Message not found"}}"#,
        extra_of(&by_date)
    ));
    let around = fx.last_request("getChatHistory");
    assert_eq!(around["from_message_id"], 1i64 << 20);
    let messages = [71i64, 70]
        .iter()
        .map(|id| text_message(id << 20, 5, "m"))
        .collect::<Vec<_>>()
        .join(",");
    fx.feed(&format!(
        r#"{{"@type":"messages","@extra":"{}","total_count":2,"messages":[{messages}]}}"#,
        extra_of(&around)
    ));
    assert_eq!(
        fx.driver.session.search.chat_search.jump,
        ChatSearchJump::Ready {
            message_id: MessageId(70 << 20)
        }
    );
}

#[test]
fn global_search_filters_reach_search_messages() {
    use crate::search_filters::{
        GlobalSearchFilters, SearchChatType, SearchDateRange, SearchMediaKind,
    };
    let mut fx = Fixture::new();
    commit_typed_search(&mut fx.driver, "dune");
    fx.driver
        .set_search_filters(GlobalSearchFilters {
            chat_type: SearchChatType::Channels,
            media: SearchMediaKind::Files,
            date: SearchDateRange::Week,
            ..GlobalSearchFilters::default()
        })
        .unwrap();
    let search = fx.last_request("searchMessages");
    assert_eq!(search["query"], "dune");
    assert_eq!(
        search["chat_type_filter"]["@type"],
        "searchMessagesChatTypeFilterChannel"
    );
    assert_eq!(search["filter"]["@type"], "searchMessagesFilterDocument");
    assert!(search["min_date"].as_i64().unwrap() > 0);
    assert_eq!(search["max_date"], 0);
    // Closing the search drops the filters.
    fx.driver.close_search();
    assert!(fx.driver.session.search.search.filters.is_default());
}
