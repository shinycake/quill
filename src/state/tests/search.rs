//! State reducer tests: search.
use super::common::*;
use super::*;

#[test]
fn inline_query_first_page_loads_slot() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.inline_query = Some(InlineQuerySlot {
        chat_id: ChatId(1),
        bot_user_id: 77,
        query: "@gif cats".to_string(),
        fetch: InlineQueryFetch::Loading,
    });
    let extra = session.request(
        RequestPurpose::GetInlineQueryResults {
            chat_id: ChatId(1),
            bot_user_id: 77,
            first_page: true,
        },
        Some(ChatId(1)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"inlineQueryResults","@extra":"{}","inline_query_id":9001,"button":null,"results":[{{"@type":"inlineQueryResultArticle","id":"a1","title":"An article","description":"Desc"}}],"next_offset":"25"}}"#,
            extra.0,
        ),
    );
    match &session.inline_query {
        Some(slot) => match &slot.fetch {
            InlineQueryFetch::Loaded {
                inline_query_id,
                button,
                results,
                next_offset,
            } => {
                assert_eq!(*inline_query_id, 9001);
                assert_eq!(*button, None);
                assert_eq!(next_offset, "25");
                assert_eq!(results.len(), 1);
                assert_eq!(results[0].id, "a1");
                assert_eq!(results[0].kind, "article");
                assert_eq!(results[0].title, "An article");
                assert_eq!(results[0].description, "Desc");
            }
            other => panic!("unexpected {other:?}"),
        },
        None => panic!("inline query slot missing"),
    }
}

#[test]
fn inline_query_pagination_appends() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.inline_query = Some(InlineQuerySlot {
        chat_id: ChatId(1),
        bot_user_id: 77,
        query: "@gif cats".to_string(),
        fetch: InlineQueryFetch::Loaded {
            inline_query_id: 9001,
            button: None,
            results: vec![InlineQueryResultSummary {
                id: "a1".to_string(),
                kind: "article".to_string(),
                title: "An article".to_string(),
                description: String::new(),
            }],
            next_offset: "25".to_string(),
        },
    });
    let extra = session.request(
        RequestPurpose::GetInlineQueryResults {
            chat_id: ChatId(1),
            bot_user_id: 77,
            first_page: false,
        },
        Some(ChatId(1)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"inlineQueryResults","@extra":"{}","inline_query_id":9002,"button":null,"results":[{{"@type":"inlineQueryResultArticle","id":"a1","title":"An article"}},{{"@type":"inlineQueryResultPhoto","id":"p1","title":"A photo"}}],"next_offset":"50"}}"#,
            extra.0,
        ),
    );
    match &session.inline_query {
        Some(slot) => match &slot.fetch {
            InlineQueryFetch::Loaded {
                inline_query_id,
                results,
                next_offset,
                ..
            } => {
                assert_eq!(*inline_query_id, 9002);
                assert_eq!(next_offset, "50");
                let ids: Vec<&str> = results.iter().map(|r| r.id.as_str()).collect();
                assert_eq!(ids, vec!["a1", "p1"]);
                assert_eq!(results[1].kind, "photo");
            }
            other => panic!("unexpected {other:?}"),
        },
        None => panic!("inline query slot missing"),
    }
}

#[test]
fn inline_query_first_page_error_fails_slot() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.inline_query = Some(InlineQuerySlot {
        chat_id: ChatId(1),
        bot_user_id: 77,
        query: "@gif cats".to_string(),
        fetch: InlineQueryFetch::Loading,
    });
    let extra = session.request(
        RequestPurpose::GetInlineQueryResults {
            chat_id: ChatId(1),
            bot_user_id: 77,
            first_page: true,
        },
        Some(ChatId(1)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"BOT_INLINE_DISABLED"}}"#,
            extra.0,
        ),
    );
    match &session.inline_query {
        Some(slot) => match &slot.fetch {
            InlineQueryFetch::Failed(line) => {
                assert!(line.contains("Could not load inline results"), "{line}");
            }
            other => panic!("unexpected {other:?}"),
        },
        None => panic!("inline query slot missing"),
    }
}

#[test]
fn inline_query_pagination_error_keeps_loaded_page() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.inline_query = Some(InlineQuerySlot {
        chat_id: ChatId(1),
        bot_user_id: 77,
        query: "@gif cats".to_string(),
        fetch: InlineQueryFetch::Loaded {
            inline_query_id: 9001,
            button: None,
            results: vec![InlineQueryResultSummary {
                id: "a1".to_string(),
                kind: "article".to_string(),
                title: "An article".to_string(),
                description: String::new(),
            }],
            next_offset: "25".to_string(),
        },
    });
    let extra = session.request(
        RequestPurpose::GetInlineQueryResults {
            chat_id: ChatId(1),
            bot_user_id: 77,
            first_page: false,
        },
        Some(ChatId(1)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"INTERNAL"}}"#,
            extra.0,
        ),
    );
    match &session.inline_query {
        Some(slot) => match &slot.fetch {
            InlineQueryFetch::Loaded { results, .. } => {
                assert_eq!(results.len(), 1);
                assert_eq!(results[0].id, "a1");
            }
            other => panic!("unexpected {other:?}"),
        },
        None => panic!("inline query slot missing"),
    }
}

#[test]
fn search_chats_and_messages_happy_path() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatPosition","chat_id":11,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"4","is_pinned":false}}"#,
    );
    session.open_search();
    let search_gen = session.search.begin_query("hello");
    let chats_extra = session.request_search(RequestPurpose::SearchChats, search_gen);
    let messages_extra = session.request_search(RequestPurpose::SearchMessages, search_gen);
    let public_extra = session.request_search(RequestPurpose::SearchPublicChats, search_gen);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[11]}}"#,
            chats_extra.0
        ),
    );
    assert_eq!(session.search.status, SearchStatus::Searching);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[42]}}"#,
            public_extra.0
        ),
    );
    assert_eq!(session.search.status, SearchStatus::Searching);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundMessages","@extra":"{}","total_count":1,"next_offset":"","messages":[{{"id":101,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"CANARY_SEARCH_hi","entities":[]}}}}}}]}}"#,
            messages_extra.0
        ),
    );
    assert_eq!(session.search.status, SearchStatus::Ready);
    assert_eq!(session.search.chat_ids, vec![ChatId(11)]);
    assert_eq!(session.search.public_chat_ids, vec![ChatId(42)]);
    assert_eq!(session.search.messages.len(), 1);
    assert_eq!(session.search.messages[0].preview, "CANARY_SEARCH_hi");
    session.promote_search_message(ChatId(11), MessageId(101));
    assert!(
        session
            .histories
            .get(&11)
            .unwrap()
            .messages
            .contains_key(&101)
    );
    session.close_search();
    assert_eq!(session.search.status, SearchStatus::Closed);
    assert!(session.search.query.is_empty());
    assert!(!sink.rendered().contains("CANARY_SEARCH"));
}

#[test]
fn search_empty_and_error_and_stale_generation() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let search_gen = session.search.begin_query("zzz");
    let chats_extra = session.request_search(RequestPurpose::SearchChats, search_gen);
    let messages_extra = session.request_search(RequestPurpose::SearchMessages, search_gen);
    let public_extra = session.request_search(RequestPurpose::SearchPublicChats, search_gen);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":0,"chat_ids":[]}}"#,
            chats_extra.0
        ),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundMessages","@extra":"{}","total_count":0,"next_offset":"","messages":[]}}"#,
            messages_extra.0
        ),
    );
    // Phase 7.2: still waiting on the public leg.
    assert_eq!(session.search.status, SearchStatus::Searching);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":0,"chat_ids":[]}}"#,
            public_extra.0
        ),
    );
    assert_eq!(session.search.status, SearchStatus::Empty);

    let stale = session.search.begin_query("old");
    let stale_chats = session.request_search(RequestPurpose::SearchChats, stale);
    let stale_messages = session.request_search(RequestPurpose::SearchMessages, stale);
    let fresh = session.search.begin_query("new");
    let _fresh_chats = session.request_search(RequestPurpose::SearchChats, fresh);
    let fresh_messages = session.request_search(RequestPurpose::SearchMessages, fresh);
    let fresh_public = session.request_search(RequestPurpose::SearchPublicChats, fresh);
    let _ = fresh_public;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[99]}}"#,
            stale_chats.0
        ),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundMessages","@extra":"{}","total_count":1,"next_offset":"","messages":[{{"id":1,"chat_id":99,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"stale","entities":[]}}}}}}]}}"#,
            stale_messages.0
        ),
    );
    assert_eq!(session.search.status, SearchStatus::Searching);
    assert!(session.search.chat_ids.is_empty());
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","code":400,"message":"CANARY_SEARCH_ERR","@extra":"{}"}}"#,
            fresh_messages.0
        ),
    );
    let search_gen = session.search.generation;
    let chats_extra = session.request_search(RequestPurpose::SearchChats, search_gen);
    let public_extra = session.request_search(RequestPurpose::SearchPublicChats, search_gen);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","code":400,"message":"CANARY_SEARCH_ERR2","@extra":"{}"}}"#,
            chats_extra.0
        ),
    );
    assert_eq!(session.search.status, SearchStatus::Searching);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","code":400,"message":"CANARY_SEARCH_ERR3","@extra":"{}"}}"#,
            public_extra.0
        ),
    );
    assert_eq!(session.search.status, SearchStatus::Failed);
    assert!(!sink.rendered().contains("CANARY_SEARCH_ERR"));
}

#[test]
fn search_recently_found_chats_empty_query() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatPosition","chat_id":11,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"4","is_pinned":false}}"#,
    );
    let search_gen = session.search.begin_recents();
    assert!(session.search.recents);
    let extra = session.request_search(RequestPurpose::SearchRecentlyFoundChats, search_gen);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[11]}}"#,
            extra.0
        ),
    );
    assert_eq!(session.search.status, SearchStatus::Ready);
    assert_eq!(session.search.chat_ids, vec![ChatId(11)]);
    assert!(session.search.messages.is_empty());
    let empty_gen = session.search.begin_recents();
    let empty_extra = session.request_search(RequestPurpose::SearchRecentlyFoundChats, empty_gen);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":0,"chat_ids":[]}}"#,
            empty_extra.0
        ),
    );
    assert_eq!(session.search.status, SearchStatus::Idle);
    assert!(session.search.recents);
}

#[test]
fn chat_search_happy_empty_stale_and_jump() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hello already loaded.","entities":[]}}}}"#,
    );
    session.open_chat(ChatId(11));
    assert!(session.open_chat_search());
    let search_gen = session.chat_search.begin_query("hello");
    let extra =
        session.request_chat_search(RequestPurpose::SearchChatMessages, ChatId(11), search_gen);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":2,"next_from_message_id":40,"messages":[{{"id":101,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"CANARY_CHAT_hi","entities":[]}}}}}},{{"id":90,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"older hello","entities":[]}}}}}}]}}"#,
            extra.0
        ),
    );
    assert_eq!(session.chat_search.status, SearchStatus::Ready);
    assert_eq!(session.chat_search.hits.len(), 2);
    assert_eq!(session.chat_search.selected, Some(0));
    assert_eq!(session.chat_search.hits[0].preview, "CANARY_CHAT_hi");
    assert_eq!(
        session.begin_chat_search_jump(MessageId(101)),
        ChatSearchJumpNeed::AlreadyReady
    );
    assert_eq!(
        session.chat_search.jump,
        ChatSearchJump::Ready {
            message_id: MessageId(101)
        }
    );
    assert_eq!(
        session.begin_chat_search_jump(MessageId(90)),
        ChatSearchJumpNeed::LoadAround
    );
    let around = session.request_history_around(ChatId(11), MessageId(90));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":2,"messages":[{{"id":90,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"older hello","entities":[]}}}}}},{{"id":89,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"neighbor","entities":[]}}}}}}]}}"#,
            around.0
        ),
    );
    assert_eq!(
        session.chat_search.jump,
        ChatSearchJump::Ready {
            message_id: MessageId(90)
        }
    );
    assert!(session.histories.get(&11).unwrap().contains(MessageId(90)));
    assert!(session.histories.get(&11).unwrap().contains(MessageId(89)));
    assert!(session.histories.get(&11).unwrap().contains(MessageId(101)));

    let empty_gen = session.chat_search.begin_query("zzz");
    let empty_extra =
        session.request_chat_search(RequestPurpose::SearchChatMessages, ChatId(11), empty_gen);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":0,"next_from_message_id":0,"messages":[]}}"#,
            empty_extra.0
        ),
    );
    assert_eq!(session.chat_search.status, SearchStatus::Empty);

    let stale = session.chat_search.begin_query("old");
    let stale_extra =
        session.request_chat_search(RequestPurpose::SearchChatMessages, ChatId(11), stale);
    let fresh = session.chat_search.begin_query("new");
    let fresh_extra =
        session.request_chat_search(RequestPurpose::SearchChatMessages, ChatId(11), fresh);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":1,"next_from_message_id":0,"messages":[{{"id":1,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"stale","entities":[]}}}}}}]}}"#,
            stale_extra.0
        ),
    );
    assert_eq!(session.chat_search.status, SearchStatus::Searching);
    assert!(session.chat_search.hits.is_empty());
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","code":400,"message":"CANARY_CHAT_ERR","@extra":"{}"}}"#,
            fresh_extra.0
        ),
    );
    assert_eq!(session.chat_search.status, SearchStatus::Failed);
    session.close_chat_search();
    assert_eq!(session.chat_search.status, SearchStatus::Closed);
    assert!(session.histories.get(&11).unwrap().contains(MessageId(101)));
    assert!(!sink.rendered().contains("CANARY_CHAT"));
}

#[test]
fn chat_search_jump_missing_deleted_and_inaccessible() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":101,"chat_id":11,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hello","entities":[]}}}}"#,
    );
    session.open_chat(ChatId(11));
    assert!(session.open_chat_search());
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateDeleteMessages","chat_id":11,"message_ids":[70],"is_permanent":true,"from_cache":false}"#,
    );
    session.chat_search.hits.push(SearchMessageHit {
        sender: None,
        chat_id: ChatId(11),
        message_id: MessageId(70),
        preview: "gone".into(),
        is_outgoing: false,
        content: crate::telegram::envelope::MessageContent::Text("gone".into()),
        author_signature: None,
        reply_to: None,
        forward_info: None,
        interaction_info: None,
        is_pinned: false,
        media_album_id: 0,
        reply_markup: None,
        self_destruct: None,
        auto_delete: None,
        date: 0,
    });
    assert_eq!(
        session.begin_chat_search_jump(MessageId(70)),
        ChatSearchJumpNeed::Missing
    );
    assert_eq!(
        session.chat_search.jump,
        ChatSearchJump::Missing {
            message_id: MessageId(70)
        }
    );

    session.chat_search.hits.push(SearchMessageHit {
        sender: None,
        chat_id: ChatId(11),
        message_id: MessageId(80),
        preview: "ghost".into(),
        is_outgoing: false,
        content: crate::telegram::envelope::MessageContent::Text("ghost".into()),
        author_signature: None,
        reply_to: None,
        forward_info: None,
        interaction_info: None,
        is_pinned: false,
        media_album_id: 0,
        reply_markup: None,
        self_destruct: None,
        auto_delete: None,
        date: 0,
    });
    assert_eq!(
        session.begin_chat_search_jump(MessageId(80)),
        ChatSearchJumpNeed::LoadAround
    );
    let around = session.request_history_around(ChatId(11), MessageId(80));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":1,"messages":[{{"id":79,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"neighbor only","entities":[]}}}}}}]}}"#,
            around.0
        ),
    );
    assert_eq!(
        session.chat_search.jump,
        ChatSearchJump::Missing {
            message_id: MessageId(80)
        }
    );
    assert!(!session.histories.get(&11).unwrap().contains(MessageId(80)));

    let stale_around = session.request_history_around(ChatId(11), MessageId(80));
    session.chat_search.generation = session.chat_search.generation.saturating_add(1);
    assert_eq!(
        session.begin_chat_search_jump(MessageId(101)),
        ChatSearchJumpNeed::AlreadyReady
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":1,"messages":[{{"id":80,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"late","entities":[]}}}}}}]}}"#,
            stale_around.0
        ),
    );
    assert_eq!(
        session.chat_search.jump,
        ChatSearchJump::Ready {
            message_id: MessageId(101)
        }
    );
    assert!(!sink.rendered().contains("CANARY"));
}

#[test]
fn public_search_results_accepted() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_search();
    let search_gen = session.search.begin_query("quill");
    let public_extra = session.request_search(RequestPurpose::SearchPublicChats, search_gen);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[42]}}"#,
            public_extra.0
        ),
    );
    assert_eq!(session.search.public_chat_ids, vec![ChatId(42)]);
    // Still waiting on `searchChats` / `searchMessages` → still Searching.
    assert_eq!(session.search.status, SearchStatus::Searching);
    let chats_extra = session.request_search(RequestPurpose::SearchChats, search_gen);
    let messages_extra = session.request_search(RequestPurpose::SearchMessages, search_gen);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":0,"chat_ids":[]}}"#,
            chats_extra.0
        ),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundMessages","@extra":"{}","total_count":0,"messages":[],"next_offset":""}}"#,
            messages_extra.0
        ),
    );
    assert_eq!(session.search.status, SearchStatus::Ready);
}

#[test]
fn public_search_error_resolves_status() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_search();
    let search_gen = session.search.begin_query("zzz");
    let public_extra = session.request_search(RequestPurpose::SearchPublicChats, search_gen);
    let chats_extra = session.request_search(RequestPurpose::SearchChats, search_gen);
    let messages_extra = session.request_search(RequestPurpose::SearchMessages, search_gen);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"QUERY_TOO_SHORT"}}"#,
            public_extra.0
        ),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":0,"chat_ids":[]}}"#,
            chats_extra.0
        ),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundMessages","@extra":"{}","total_count":0,"messages":[],"next_offset":""}}"#,
            messages_extra.0
        ),
    );
    assert_eq!(session.search.status, SearchStatus::Failed);
    assert!(session.search.public_chat_ids.is_empty());
}

#[test]
fn pinned_list_fills_newest_first_and_drops_unpins() {
    // The pinned bar's list: a `searchMessagesFilterPinned` page lands
    // newest first; an unpin or delete drops the row at once.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetPinnedMessages, Some(ChatId(16)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":3,"next_from_message_id":0,"messages":[{{"id":40,"chat_id":16,"is_outgoing":false,"is_pinned":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"older","entities":[]}}}}}},{{"id":60,"chat_id":16,"is_outgoing":false,"is_pinned":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"newest","entities":[]}}}}}},{{"id":50,"chat_id":16,"is_outgoing":false,"is_pinned":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"middle","entities":[]}}}}}}]}}"#,
            extra.0
        ),
    );
    let ids: Vec<i64> = session
        .pinned_list(ChatId(16))
        .iter()
        .map(|message| message.id.0)
        .collect();
    assert_eq!(ids, vec![60, 50, 40]);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateMessageIsPinned","chat_id":16,"message_id":50,"is_pinned":false}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateDeleteMessages","chat_id":16,"message_ids":[60],"is_permanent":true,"from_cache":false}"#,
    );
    let ids: Vec<i64> = session
        .pinned_list(ChatId(16))
        .iter()
        .map(|message| message.id.0)
        .collect();
    assert_eq!(ids, vec![40]);
}
