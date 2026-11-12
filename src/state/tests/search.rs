//! State reducer tests: search.
use super::common::*;
use super::*;

#[test]
fn inline_query_first_page_loads_slot() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(
        RequestPurpose::Bots(BotsPurpose::GetInlineQueryResults {
            chat_id: ChatId(1),
            bot_user_id: 77,
            first_page: true,
        }),
        Some(ChatId(1)),
    );
    session.bots.inline_query = Some(InlineQuerySlot {
        chat_id: ChatId(1),
        bot_user_id: 77,
        query: "@gif cats".to_string(),
        fetch: InlineQueryFetch::Loading { request: extra },
    });
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"inlineQueryResults","@extra":"{}","inline_query_id":9001,"button":null,"results":[{{"@type":"inlineQueryResultArticle","id":"a1","title":"An article","description":"Desc"}}],"next_offset":"25"}}"#,
            extra.0,
        ),
    );
    match &session.bots.inline_query {
        Some(slot) => match &slot.fetch {
            InlineQueryFetch::Loaded {
                inline_query_id,
                button,
                results,
                next_offset,
                ..
            } => {
                assert_eq!(*inline_query_id, 9001);
                assert_eq!(*button, None);
                assert_eq!(next_offset, "25");
                assert_eq!(results.len(), 1);
                assert_eq!(results[0].id, "a1");
                assert_eq!(results[0].kind.name(), "article");
                assert_eq!(results[0].title, "An article");
                assert_eq!(results[0].description, "Desc");
            }
            other => panic!("unexpected {other:?}"),
        },
        None => panic!("inline query slot missing"),
    }
}

#[test]
fn inline_query_answer_of_an_older_request_is_dropped() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let ask = |session: &mut Session| {
        session.request(
            RequestPurpose::Bots(BotsPurpose::GetInlineQueryResults {
                chat_id: ChatId(1),
                bot_user_id: 77,
                first_page: true,
            }),
            Some(ChatId(1)),
        )
    };
    let old = ask(&mut session);
    let new = ask(&mut session);
    session.bots.inline_query = Some(InlineQuerySlot {
        chat_id: ChatId(1),
        bot_user_id: 77,
        query: "second".to_string(),
        fetch: InlineQueryFetch::Loading { request: new },
    });
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"inlineQueryResults","@extra":"{}","inline_query_id":1,"button":null,"results":[],"next_offset":""}}"#,
            old.0,
        ),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"X"}}"#,
            old.0,
        ),
    );
    assert!(matches!(
        session.bots.inline_query.as_ref().map(|s| &s.fetch),
        Some(InlineQueryFetch::Loading { request }) if *request == new
    ));
}

#[test]
fn inline_query_pagination_appends() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(
        RequestPurpose::Bots(BotsPurpose::GetInlineQueryResults {
            chat_id: ChatId(1),
            bot_user_id: 77,
            first_page: false,
        }),
        Some(ChatId(1)),
    );
    session.bots.inline_query = Some(InlineQuerySlot {
        chat_id: ChatId(1),
        bot_user_id: 77,
        query: "@gif cats".to_string(),
        fetch: InlineQueryFetch::Loaded {
            inline_query_id: 9001,
            button: None,
            results: vec![InlineQueryResultSummary {
                id: "a1".to_string(),
                kind: "article".into(),
                title: "An article".to_string(),
                description: String::new(),
                ..Default::default()
            }],
            next_offset: "25".to_string(),
            more: InlineMore::Asking { request: extra },
        },
    });
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"inlineQueryResults","@extra":"{}","inline_query_id":9002,"button":null,"results":[{{"@type":"inlineQueryResultArticle","id":"a1","title":"An article"}},{{"@type":"inlineQueryResultPhoto","id":"p1","title":"A photo"}}],"next_offset":"50"}}"#,
            extra.0,
        ),
    );
    match &session.bots.inline_query {
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
                assert_eq!(results[1].kind.name(), "photo");
            }
            other => panic!("unexpected {other:?}"),
        },
        None => panic!("inline query slot missing"),
    }
}

/// 10-094: a first page's answer fills the slot only when it is the request
/// the slot waits for.
#[test]
fn inline_query_first_page_of_an_older_request_is_dropped() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let purpose = RequestPurpose::Bots(BotsPurpose::GetInlineQueryResults {
        chat_id: ChatId(1),
        bot_user_id: 77,
        first_page: true,
    });
    let old = session.request(purpose, Some(ChatId(1)));
    let current = session.request(purpose, Some(ChatId(1)));
    session.bots.inline_query = Some(InlineQuerySlot {
        chat_id: ChatId(1),
        bot_user_id: 77,
        query: "@gif dogs".to_string(),
        fetch: InlineQueryFetch::Loading { request: current },
    });
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"inlineQueryResults","@extra":"{}","inline_query_id":1,"button":null,"results":[],"next_offset":""}}"#,
            old.0,
        ),
    );
    assert_eq!(
        session.bots.inline_query.as_ref().map(|slot| &slot.fetch),
        Some(&InlineQueryFetch::Loading { request: current })
    );
}

#[test]
fn inline_query_first_page_error_fails_slot() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(
        RequestPurpose::Bots(BotsPurpose::GetInlineQueryResults {
            chat_id: ChatId(1),
            bot_user_id: 77,
            first_page: true,
        }),
        Some(ChatId(1)),
    );
    session.bots.inline_query = Some(InlineQuerySlot {
        chat_id: ChatId(1),
        bot_user_id: 77,
        query: "@gif cats".to_string(),
        fetch: InlineQueryFetch::Loading { request: extra },
    });
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"BOT_INLINE_DISABLED"}}"#,
            extra.0,
        ),
    );
    match &session.bots.inline_query {
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
    let extra = session.request(
        RequestPurpose::Bots(BotsPurpose::GetInlineQueryResults {
            chat_id: ChatId(1),
            bot_user_id: 77,
            first_page: false,
        }),
        Some(ChatId(1)),
    );
    session.bots.inline_query = Some(InlineQuerySlot {
        chat_id: ChatId(1),
        bot_user_id: 77,
        query: "@gif cats".to_string(),
        fetch: InlineQueryFetch::Loaded {
            inline_query_id: 9001,
            button: None,
            results: vec![InlineQueryResultSummary {
                id: "a1".to_string(),
                kind: "article".into(),
                title: "An article".to_string(),
                description: String::new(),
                ..Default::default()
            }],
            next_offset: "25".to_string(),
            more: InlineMore::Asking { request: extra },
        },
    });
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"INTERNAL"}}"#,
            extra.0,
        ),
    );
    match &session.bots.inline_query {
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
    session.search.search.begin_query("hello");
    let chats_extra = session.request_search(RequestPurpose::SearchChats);
    let messages_extra = session.request_search(RequestPurpose::SearchMessages);
    let public_extra = session.request_search(RequestPurpose::SearchPublicChats);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[11]}}"#,
            chats_extra.0
        ),
    );
    assert_eq!(session.search.search.status(), SearchStatus::Searching);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[42]}}"#,
            public_extra.0
        ),
    );
    assert_eq!(session.search.search.status(), SearchStatus::Searching);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundMessages","@extra":"{}","total_count":1,"next_offset":"","messages":[{{"id":101,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"CANARY_SEARCH_hi","entities":[]}}}}}}]}}"#,
            messages_extra.0
        ),
    );
    assert_eq!(session.search.search.status(), SearchStatus::Ready);
    assert_eq!(session.search.search.chat_ids, vec![ChatId(11)]);
    assert_eq!(session.search.search.public_chat_ids, vec![ChatId(42)]);
    assert_eq!(session.search.search.messages.len(), 1);
    assert_eq!(
        session.search.search.messages[0].preview,
        "CANARY_SEARCH_hi"
    );
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
    assert_eq!(session.search.search.status(), SearchStatus::Closed);
    assert!(session.search.search.query.is_empty());
    assert!(!sink.rendered().contains("CANARY_SEARCH"));
}

#[test]
fn search_empty_and_error_and_stale_generation() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.search.search.begin_query("zzz");
    let chats_extra = session.request_search(RequestPurpose::SearchChats);
    let messages_extra = session.request_search(RequestPurpose::SearchMessages);
    let public_extra = session.request_search(RequestPurpose::SearchPublicChats);
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
    assert_eq!(session.search.search.status(), SearchStatus::Searching);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":0,"chat_ids":[]}}"#,
            public_extra.0
        ),
    );
    assert_eq!(session.search.search.status(), SearchStatus::Empty);

    session.search.search.begin_query("old");
    let stale_chats = session.request_search(RequestPurpose::SearchChats);
    let stale_messages = session.request_search(RequestPurpose::SearchMessages);
    session.search.search.begin_query("new");
    let _fresh_chats = session.request_search(RequestPurpose::SearchChats);
    let fresh_messages = session.request_search(RequestPurpose::SearchMessages);
    let fresh_public = session.request_search(RequestPurpose::SearchPublicChats);
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
    assert_eq!(session.search.search.status(), SearchStatus::Searching);
    assert!(session.search.search.chat_ids.is_empty());
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","code":400,"message":"CANARY_SEARCH_ERR","@extra":"{}"}}"#,
            fresh_messages.0
        ),
    );
    let chats_extra = session.request_search(RequestPurpose::SearchChats);
    let public_extra = session.request_search(RequestPurpose::SearchPublicChats);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","code":400,"message":"CANARY_SEARCH_ERR2","@extra":"{}"}}"#,
            chats_extra.0
        ),
    );
    assert_eq!(session.search.search.status(), SearchStatus::Searching);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","code":400,"message":"CANARY_SEARCH_ERR3","@extra":"{}"}}"#,
            public_extra.0
        ),
    );
    assert_eq!(session.search.search.status(), SearchStatus::Failed);
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
    session.search.search.begin_recents();
    assert!(session.search.search.is_recents());
    let extra = session.request_search(RequestPurpose::SearchRecentlyFoundChats);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[11]}}"#,
            extra.0
        ),
    );
    assert_eq!(session.search.search.status(), SearchStatus::Ready);
    assert_eq!(session.search.search.chat_ids, vec![ChatId(11)]);
    assert!(session.search.search.messages.is_empty());
    session.search.search.begin_recents();
    let empty_extra = session.request_search(RequestPurpose::SearchRecentlyFoundChats);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":0,"chat_ids":[]}}"#,
            empty_extra.0
        ),
    );
    assert_eq!(session.search.search.status(), SearchStatus::Idle);
    assert!(session.search.search.is_recents());
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
    session.search.chat_search.begin_query("hello");
    let extra = session.request_chat_search(RequestPurpose::SearchChatMessages, ChatId(11));
    session.search.chat_search.searching(extra);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":2,"next_from_message_id":40,"messages":[{{"id":101,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"CANARY_CHAT_hi","entities":[]}}}}}},{{"id":90,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"older hello","entities":[]}}}}}}]}}"#,
            extra.0
        ),
    );
    assert_eq!(session.search.chat_search.status(), SearchStatus::Ready);
    assert_eq!(session.search.chat_search.hits().len(), 2);
    assert_eq!(session.search.chat_search.selected(), Some(0));
    assert_eq!(
        session.search.chat_search.hits()[0].preview,
        "CANARY_CHAT_hi"
    );
    assert_eq!(
        session.begin_chat_search_jump(MessageId(101)),
        ChatSearchJumpNeed::AlreadyReady
    );
    assert_eq!(
        session.search.chat_search.jump,
        ChatSearchJump::Ready {
            message_id: MessageId(101)
        }
    );
    let ChatSearchJumpNeed::LoadAround {
        request: around, ..
    } = session.begin_chat_search_jump(MessageId(90))
    else {
        panic!("90 is outside the loaded window");
    };
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
        session.search.chat_search.jump,
        ChatSearchJump::Ready {
            message_id: MessageId(90)
        }
    );
    assert!(session.histories.get(&11).unwrap().contains(MessageId(90)));
    assert!(session.histories.get(&11).unwrap().contains(MessageId(89)));
    // 90 is not adjacent to 101: the page replaced the window instead of
    // joining it across a gap.
    assert!(!session.histories.get(&11).unwrap().contains(MessageId(101)));

    session.search.chat_search.begin_query("zzz");
    let empty_extra = session.request_chat_search(RequestPurpose::SearchChatMessages, ChatId(11));
    session.search.chat_search.searching(empty_extra);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":0,"next_from_message_id":0,"messages":[]}}"#,
            empty_extra.0
        ),
    );
    assert_eq!(session.search.chat_search.status(), SearchStatus::Empty);

    session.search.chat_search.begin_query("old");
    let stale_extra = session.request_chat_search(RequestPurpose::SearchChatMessages, ChatId(11));
    session.search.chat_search.searching(stale_extra);
    session.search.chat_search.begin_query("new");
    let fresh_extra = session.request_chat_search(RequestPurpose::SearchChatMessages, ChatId(11));
    session.search.chat_search.searching(fresh_extra);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":1,"next_from_message_id":0,"messages":[{{"id":1,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"stale","entities":[]}}}}}}]}}"#,
            stale_extra.0
        ),
    );
    assert_eq!(session.search.chat_search.status(), SearchStatus::Searching);
    assert!(session.search.chat_search.hits().is_empty());
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","code":400,"message":"CANARY_CHAT_ERR","@extra":"{}"}}"#,
            fresh_extra.0
        ),
    );
    assert_eq!(session.search.chat_search.status(), SearchStatus::Failed);
    session.close_chat_search();
    assert_eq!(session.search.chat_search.status(), SearchStatus::Closed);
    assert!(session.histories.get(&11).unwrap().contains(MessageId(90)));
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
    session
        .search
        .chat_search
        .open_mut()
        .unwrap()
        .hits
        .push(SearchMessageHit {
            sender: None,
            chat_id: ChatId(11),
            message_id: MessageId(70),
            preview: "gone".into(),
            is_outgoing: false,
            content: crate::telegram::envelope::MessageContent::Text("gone".into()),
            author_signature: None,
            reply_to: None,
            forward_info: None,
            extras: Default::default(),
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
        session.search.chat_search.jump,
        ChatSearchJump::Missing {
            message_id: MessageId(70)
        }
    );

    session
        .search
        .chat_search
        .open_mut()
        .unwrap()
        .hits
        .push(SearchMessageHit {
            sender: None,
            chat_id: ChatId(11),
            message_id: MessageId(80),
            preview: "ghost".into(),
            is_outgoing: false,
            content: crate::telegram::envelope::MessageContent::Text("ghost".into()),
            author_signature: None,
            reply_to: None,
            forward_info: None,
            extras: Default::default(),
            interaction_info: None,
            is_pinned: false,
            media_album_id: 0,
            reply_markup: None,
            self_destruct: None,
            auto_delete: None,
            date: 0,
        });
    let ChatSearchJumpNeed::LoadAround {
        request: around, ..
    } = session.begin_chat_search_jump(MessageId(80))
    else {
        panic!("80 is outside the loaded window");
    };
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
        session.search.chat_search.jump,
        ChatSearchJump::Missing {
            message_id: MessageId(80)
        }
    );
    assert!(!session.histories.get(&11).unwrap().contains(MessageId(80)));

    let stale_around = session.request_history_around(ChatId(11), MessageId(80));
    assert_eq!(
        session.begin_chat_search_jump(MessageId(79)),
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
        session.search.chat_search.jump,
        ChatSearchJump::Ready {
            message_id: MessageId(79)
        }
    );
    assert!(!sink.rendered().contains("CANARY"));
}

#[test]
fn public_search_results_accepted() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_search();
    session.search.search.begin_query("quill");
    let public_extra = session.request_search(RequestPurpose::SearchPublicChats);
    let chats_extra = session.request_search(RequestPurpose::SearchChats);
    let messages_extra = session.request_search(RequestPurpose::SearchMessages);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[42]}}"#,
            public_extra.0
        ),
    );
    assert_eq!(session.search.search.public_chat_ids, vec![ChatId(42)]);
    // Still waiting on `searchChats` / `searchMessages` → still Searching.
    assert_eq!(session.search.search.status(), SearchStatus::Searching);
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
    assert_eq!(session.search.search.status(), SearchStatus::Ready);
}

#[test]
fn public_search_error_resolves_status() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_search();
    session.search.search.begin_query("zzz");
    let public_extra = session.request_search(RequestPurpose::SearchPublicChats);
    let chats_extra = session.request_search(RequestPurpose::SearchChats);
    let messages_extra = session.request_search(RequestPurpose::SearchMessages);
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
    assert_eq!(session.search.search.status(), SearchStatus::Failed);
    assert!(session.search.search.public_chat_ids.is_empty());
}

#[test]
fn pinned_list_fills_newest_first_and_drops_unpins() {
    // The pinned bar's list: a `searchMessagesFilterPinned` page lands
    // newest first; an unpin or delete drops the row at once.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetPinnedMessages, Some(ChatId(16)));
    let fetch = session.messages.pinned_messages.entry(16).or_default();
    fetch.begin();
    fetch.sent(extra);
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

fn loaded_inline_slot(session: &mut Session, results: Vec<InlineQueryResultSummary>) {
    session.bots.inline_query = Some(InlineQuerySlot {
        chat_id: ChatId(1),
        bot_user_id: 77,
        query: "cats".to_string(),
        fetch: InlineQueryFetch::Loaded {
            inline_query_id: 9001,
            button: Some(InlineQueryResultsButton {
                text: "Log in".into(),
                kind: crate::telegram::envelope::InlineQueryResultsButtonKind::StartBot {
                    parameter: "auth".into(),
                },
            }),
            results,
            next_offset: "25".to_string(),
            more: InlineMore::Idle,
        },
    });
}

fn inline_next_page(session: &mut Session, sink: &Arc<MemorySink>, json_results: &str, next: &str) {
    let seq = AtomicU64::new(0);
    let extra = session.request(
        RequestPurpose::Bots(BotsPurpose::GetInlineQueryResults {
            chat_id: ChatId(1),
            bot_user_id: 77,
            first_page: false,
        }),
        Some(ChatId(1)),
    );
    if let Some(InlineQuerySlot {
        fetch: InlineQueryFetch::Loaded { more, .. },
        ..
    }) = session.bots.inline_query.as_mut()
    {
        *more = InlineMore::Asking { request: extra };
    }
    apply_json(
        session,
        &seq,
        sink,
        &format!(
            r#"{{"@type":"inlineQueryResults","@extra":"{}","inline_query_id":"9002","button":null,"results":{json_results},"next_offset":"{next}"}}"#,
            extra.0,
        ),
    );
}

#[test]
fn inline_next_page_stamps_its_own_query_id_and_keeps_the_button() {
    let (mut session, sink) = session();
    loaded_inline_slot(
        &mut session,
        vec![InlineQueryResultSummary {
            id: "a1".into(),
            kind: "article".into(),
            query_id: 9001,
            ..Default::default()
        }],
    );
    inline_next_page(
        &mut session,
        &sink,
        r#"[{"@type":"inlineQueryResultArticle","id":"a2","title":"B"}]"#,
        "50",
    );
    let Some(InlineQuerySlot {
        fetch:
            InlineQueryFetch::Loaded {
                results,
                button,
                next_offset,
                ..
            },
        ..
    }) = &session.bots.inline_query
    else {
        panic!("slot not loaded");
    };
    // Each result sends with the id of the page that returned it.
    assert_eq!(
        results.iter().map(|r| r.query_id).collect::<Vec<_>>(),
        vec![9001, 9002]
    );
    assert_eq!(next_offset, "50");
    assert_eq!(
        button.as_ref().map(|b| &b.kind),
        Some(
            &crate::telegram::envelope::InlineQueryResultsButtonKind::StartBot {
                parameter: "auth".into()
            }
        )
    );
}

#[test]
fn inline_next_page_with_nothing_new_ends_the_list() {
    let (mut session, sink) = session();
    loaded_inline_slot(
        &mut session,
        vec![InlineQueryResultSummary {
            id: "a1".into(),
            kind: "article".into(),
            ..Default::default()
        }],
    );
    inline_next_page(
        &mut session,
        &sink,
        r#"[{"@type":"inlineQueryResultArticle","id":"a1","title":"again"}]"#,
        "50",
    );
    let Some(InlineQuerySlot {
        fetch: InlineQueryFetch::Loaded { next_offset, .. },
        ..
    }) = &session.bots.inline_query
    else {
        panic!("slot not loaded");
    };
    assert!(next_offset.is_empty());
}

#[test]
fn inline_result_images_are_queued_for_download() {
    let (mut session, _sink) = session();
    session.bots.inline_query = Some(InlineQuerySlot {
        chat_id: ChatId(1),
        bot_user_id: 77,
        query: "cats".to_string(),
        fetch: InlineQueryFetch::Loaded {
            inline_query_id: 1,
            button: None,
            results: vec![
                InlineQueryResultSummary {
                    id: "p".into(),
                    kind: "photo".into(),
                    media: Some(quill_media(7)),
                    ..Default::default()
                },
                // A GIF whose thumbnail is unknown yet has nothing to fetch.
                InlineQueryResultSummary {
                    id: "g".into(),
                    kind: "animation".into(),
                    media: Some(quill_media(0)),
                    ..Default::default()
                },
            ],
            next_offset: String::new(),
            more: InlineMore::Idle,
        },
    });
    assert_eq!(
        session.inline_result_file_ids_to_download(),
        vec![FileId(7)]
    );
    // The panel has its own, higher-priority pass.
    assert!(session.thumb_file_ids_to_download().is_empty());
}

fn quill_media(file_id: i32) -> crate::telegram::envelope::InlineMedia {
    crate::telegram::envelope::InlineMedia {
        file_id: FileId(file_id),
        width: 100,
        height: 50,
    }
}

#[test]
fn an_error_on_one_leg_with_results_on_another_is_ready() {
    let mut search = SearchState::default();
    search.open_field();
    search.begin_query("ada");
    search.leg_sent(RequestPurpose::SearchChats, RequestId(1));
    search.leg_sent(RequestPurpose::SearchMessages, RequestId(2));
    search.leg_sent(RequestPurpose::SearchPublicChats, RequestId(3));
    assert_eq!(search.status(), SearchStatus::Searching);
    search.accept_chats(Vec::new(), true);
    search.accept_public_chats(Vec::new(), false);
    assert_eq!(search.status(), SearchStatus::Searching);
    search.accept_messages(Vec::new(), false);
    // Only the failed leg answered: nothing to show but the failure.
    assert_eq!(search.status(), SearchStatus::Failed);
    search.accept_public_chats(vec![ChatId(9)], false);
    // A leg that found something beats the failed one.
    assert_eq!(search.status(), SearchStatus::Ready);
}

#[test]
fn status_follows_open_and_the_legs() {
    let mut search = SearchState::default();
    assert_eq!(search.status(), SearchStatus::Closed);
    search.open_field();
    assert_eq!(search.status(), SearchStatus::Idle);
    search.begin_recents();
    search.leg_sent(RequestPurpose::SearchRecentlyFoundChats, RequestId(1));
    assert_eq!(search.status(), SearchStatus::Searching);
    search.accept_chats(Vec::new(), false);
    assert_eq!(search.status(), SearchStatus::Idle);
    search.begin_query("x");
    search.begin_public_scope();
    search.leg_sent(RequestPurpose::SearchPublicPosts, RequestId(2));
    assert_eq!(search.status(), SearchStatus::Searching);
    search.accept_messages(Vec::new(), false);
    assert_eq!(search.status(), SearchStatus::Empty);
    search.close();
    assert_eq!(search.status(), SearchStatus::Closed);
}

#[test]
fn the_demo_local_filter_settles_its_status() {
    let (mut session, _sink) = session();
    session.apply_local_search_filter("zzz-no-such-chat");
    assert_eq!(session.search.search.status(), SearchStatus::Empty);
    session.apply_local_search_filter("");
    assert_eq!(session.search.search.status(), SearchStatus::Idle);
    session.close_search();
    assert_eq!(session.search.search.status(), SearchStatus::Closed);
}

fn inline_request(session: &mut Session, first_page: bool) -> RequestId {
    session.request(
        RequestPurpose::Bots(BotsPurpose::GetInlineQueryResults {
            chat_id: ChatId(1),
            bot_user_id: 77,
            first_page,
        }),
        Some(ChatId(1)),
    )
}

fn inline_page_json(extra: RequestId, id: &str) -> String {
    format!(
        r#"{{"@type":"inlineQueryResults","@extra":"{}","inline_query_id":"9","button":null,"results":[{{"@type":"inlineQueryResultArticle","id":"{id}","title":"T"}}],"next_offset":"25"}}"#,
        extra.0
    )
}

/// r2-18 05-072: the first page lands only through the request the slot
/// waits for. The answer and the error of an older query leave a newer
/// query's `Loading` alone, in either order.
#[test]
fn a_late_first_page_of_an_older_query_does_not_fill_the_new_slot() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let old = inline_request(&mut session, true);
    let new = inline_request(&mut session, true);
    session.bots.inline_query = Some(InlineQuerySlot {
        chat_id: ChatId(1),
        bot_user_id: 77,
        query: "dogs".to_string(),
        fetch: InlineQueryFetch::Loading { request: new },
    });
    apply_json(&mut session, &seq, &sink, &inline_page_json(old, "old"));
    assert_eq!(
        session.bots.inline_query.as_ref().map(|slot| &slot.fetch),
        Some(&InlineQueryFetch::Loading { request: new })
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"INTERNAL"}}"#,
            old.0
        ),
    );
    assert_eq!(
        session.bots.inline_query.as_ref().map(|slot| &slot.fetch),
        Some(&InlineQueryFetch::Loading { request: new })
    );
    apply_json(&mut session, &seq, &sink, &inline_page_json(new, "new"));
    assert!(matches!(
        session.bots.inline_query.as_ref().map(|slot| &slot.fetch),
        Some(InlineQueryFetch::Loaded { results, .. }) if results.len() == 1 && results[0].id == "new"
    ));
}

/// r2-18 05-072: a next page asked for the previous query does not append
/// to the list of the query that replaced it.
#[test]
fn a_late_next_page_of_an_older_query_is_dropped() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    loaded_inline_slot(&mut session, Vec::new());
    let stale_more = inline_request(&mut session, false);
    let fresh = inline_request(&mut session, true);
    session.bots.inline_query = Some(InlineQuerySlot {
        chat_id: ChatId(1),
        bot_user_id: 77,
        query: "dogs".to_string(),
        fetch: InlineQueryFetch::Loading { request: fresh },
    });
    apply_json(&mut session, &seq, &sink, &inline_page_json(fresh, "dog"));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &inline_page_json(stale_more, "cat"),
    );
    let Some(InlineQuerySlot {
        fetch: InlineQueryFetch::Loaded { results, more, .. },
        ..
    }) = &session.bots.inline_query
    else {
        panic!("slot not loaded");
    };
    assert_eq!(
        results.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        vec!["dog"]
    );
    assert_eq!(*more, InlineMore::Idle);
}

fn voters_request(session: &mut Session, offset: i32) -> RequestId {
    session.request(
        RequestPurpose::Messages(MessagesPurpose::GetPollVoters {
            chat_id: ChatId(1),
            message_id: MessageId(5),
            option_id: 0,
            offset,
        }),
        Some(ChatId(1)),
    )
}

fn voters_json(extra: RequestId, ids: &[i64]) -> String {
    let senders: Vec<String> = ids
        .iter()
        .map(|id| format!(r#"{{"voter_id":{{"@type":"messageSenderUser","user_id":{id}}}}}"#))
        .collect();
    format!(
        r#"{{"@type":"pollVoters","@extra":"{}","total_count":{},"voters":[{}]}}"#,
        extra.0,
        ids.len(),
        senders.join(",")
    )
}

/// r2-18 05-073: a voters page that was out when an update of the poll
/// dropped the cached pages does not become the whole list.
#[test]
fn a_late_voters_page_after_an_update_does_not_become_the_list() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let key = (1, 5, 0);
    let first = voters_request(&mut session, 0);
    session
        .messages
        .poll_voters
        .insert(key, PollVotersFetch::Loading(first));
    apply_json(&mut session, &seq, &sink, &voters_json(first, &[1, 2]));
    let more = voters_request(&mut session, 2);
    let Some(PollVotersFetch::Loaded { more: slot, .. }) =
        session.messages.poll_voters.get_mut(&key)
    else {
        panic!("first page not loaded");
    };
    *slot = VotersMore::Asking(more);
    // `updatePoll` drops the cached pages.
    session.messages.poll_voters.clear();
    apply_json(&mut session, &seq, &sink, &voters_json(more, &[3]));
    assert!(session.messages.poll_voters.is_empty());
}

/// r2-18 05-073: the first page lands by its request, and a page-next
/// error keeps the loaded list retryable.
#[test]
fn voters_pages_land_by_request_and_a_next_page_error_stays_retryable() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let key = (1, 5, 0);
    let first = voters_request(&mut session, 0);
    let other = voters_request(&mut session, 0);
    session
        .messages
        .poll_voters
        .insert(key, PollVotersFetch::Loading(first));
    apply_json(&mut session, &seq, &sink, &voters_json(other, &[9]));
    assert_eq!(
        session.messages.poll_voters.get(&key),
        Some(&PollVotersFetch::Loading(first)),
        "another request's page is dropped"
    );
    apply_json(&mut session, &seq, &sink, &voters_json(first, &[1, 2]));
    let more = voters_request(&mut session, 2);
    let Some(PollVotersFetch::Loaded { more: slot, .. }) =
        session.messages.poll_voters.get_mut(&key)
    else {
        panic!("first page not loaded");
    };
    *slot = VotersMore::Asking(more);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"INTERNAL"}}"#,
            more.0
        ),
    );
    assert!(matches!(
        session.messages.poll_voters.get(&key),
        Some(PollVotersFetch::Loaded { voters, more: VotersMore::Ready, .. }) if voters.len() == 2
    ));
}

/// r2-18 05-069: the in-chat search bar is one value: closing it, or opening
/// it on another chat, replaces what it held (sender, media tab, member
/// picker, results) instead of resetting fields one by one.
#[test]
fn the_in_chat_search_bar_is_replaced_whole_on_close_and_on_another_chat() {
    let (mut session, _sink) = session();
    let search = &mut session.search.chat_search;
    assert_eq!(search.status(), SearchStatus::Closed);
    search.open_for(ChatId(1));
    search.set_sender(Some(MessageSender::User { user_id: 5 }));
    search.set_media(crate::search_filters::SearchMediaKind::Files);
    search.set_from_picker(Some(FromPicker::default()));
    search.begin_query("cats");
    search.accept_hits(Vec::new(), 0, MessageId(0), false);
    assert_eq!(search.status(), SearchStatus::Empty);
    // The same chat again is not a new search.
    search.open_for(ChatId(1));
    assert_eq!(search.query(), "cats");
    // Another chat starts a fresh one.
    search.open_for(ChatId(2));
    assert_eq!(search.chat_id(), Some(ChatId(2)));
    assert_eq!(search.status(), SearchStatus::Idle);
    assert!(search.sender().is_none() && search.from_picker().is_none());
    assert_eq!(search.media(), crate::search_filters::SearchMediaKind::All);
    search.set_sender(Some(MessageSender::User { user_id: 6 }));
    search.close();
    assert!(!search.is_open() && search.chat_id().is_none());
    assert!(search.sender().is_none() && !search.has_criteria());
    // Nothing writes into a closed bar.
    search.set_sender(Some(MessageSender::User { user_id: 7 }));
    search.begin_query("late");
    assert_eq!(search.status(), SearchStatus::Closed);
    assert!(search.query().is_empty());
}

/// r2-18 05-085: the demo's local filter goes through the search's own
/// transitions, so it starts a new list and drops a typed query waiting.
#[test]
fn the_local_global_filter_starts_a_new_epoch() {
    let (mut session, _sink) = session();
    session.search.search.open_field();
    let before = session.search.search.epoch;
    session.search.search.typed.arm("old".to_string());
    session.apply_local_search_filter("cats");
    assert!(session.search.search.epoch > before);
    assert!(session.search.search.typed.armed().is_none());
    assert_eq!(session.search.search.query, "cats");
    assert!(session.search.search.is_open());
    session.apply_local_search_filter("  ");
    assert!(session.search.search.is_recents());
    assert!(session.search.search.query.is_empty());
}

/// r2-18 06-012: the date jump waiting for its window is part of the jump
/// stage; when its chat goes it becomes a plain jump in that stage, with no
/// second value to clear.
#[test]
fn a_date_jump_waiting_for_its_window_lives_in_the_jump_stage() {
    use crate::state::DateJumpMode;
    let (mut session, _sink) = session();
    session.open_chat(ChatId(5));
    let need = session.begin_date_jump(MessageId(50), DateJumpMode::Next);
    let ChatSearchJumpNeed::LoadAround { request, .. } = need else {
        panic!("50 is outside the loaded window");
    };
    assert_eq!(
        session.search.chat_search.jump,
        ChatSearchJump::Loading {
            request,
            message_id: MessageId(50),
            then: JumpLanding::Date {
                chat_id: ChatId(5),
                mode: DateJumpMode::Next
            }
        }
    );
    // Another chat's pieces go: this jump stays.
    session.clear_date_jump_ui(Some(ChatId(9)));
    assert!(matches!(
        session.search.chat_search.jump,
        ChatSearchJump::Loading {
            then: JumpLanding::Date { .. },
            ..
        }
    ));
    session.clear_date_jump_ui(Some(ChatId(5)));
    assert_eq!(
        session.search.chat_search.jump,
        ChatSearchJump::Loading {
            request,
            message_id: MessageId(50),
            then: JumpLanding::Message
        }
    );
}

#[test]
fn r2_an_older_pinned_fetch_neither_lands_nor_fails_over_the_newer_one() {
    // Sweep r2-19 (05-038): both orders of two requests in flight.
    for newest_first in [false, true] {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let older = session.request(RequestPurpose::GetPinnedMessages, Some(ChatId(16)));
        let newer = session.request(RequestPurpose::GetPinnedMessages, Some(ChatId(16)));
        let fetch = session.messages.pinned_messages.entry(16).or_default();
        fetch.begin();
        fetch.sent(newer);
        let error = |extra: RequestId| {
            format!(
                r#"{{"@type":"error","@extra":"{}","code":500,"message":"X"}}"#,
                extra.0
            )
        };
        let page = |extra: RequestId| {
            format!(
                r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":1,"next_from_message_id":0,"messages":[{{"id":40,"chat_id":16,"is_outgoing":false,"is_pinned":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"x","entities":[]}}}}}}]}}"#,
                extra.0
            )
        };
        if newest_first {
            apply_json(&mut session, &seq, &sink, &page(newer));
        }
        apply_json(&mut session, &seq, &sink, &error(older));
        assert_eq!(
            session.messages.pinned_messages[&16].is_loading(),
            !newest_first,
            "an older error leaves the newer fetch alone"
        );
        if !newest_first {
            apply_json(&mut session, &seq, &sink, &page(newer));
        }
        assert_eq!(session.pinned_list(ChatId(16)).len(), 1);
    }
}

/// r3-06 09-009: jump to A, jump to B, jump back to A while the first around
/// page is still out. Only the newest request's answer settles the jump.
#[test]
fn an_older_around_page_for_the_same_message_does_not_settle_a_later_jump() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":11},"unread_count":0}}"#,
    );
    session.open_chat(ChatId(11));
    let jump = |session: &mut Session, id: i64| match session.begin_chat_search_jump(MessageId(id))
    {
        ChatSearchJumpNeed::LoadAround { request, .. } => request,
        other => panic!("{other:?}"),
    };
    let first = jump(&mut session, 50);
    let _other = jump(&mut session, 60);
    let again = jump(&mut session, 50);
    assert_ne!(first, again);
    assert_eq!(
        session.search.chat_search.jump,
        ChatSearchJump::Loading {
            request: again,
            message_id: MessageId(50),
            then: JumpLanding::Message
        }
    );
    let page = |extra: RequestId| {
        format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":1,"messages":[{{"id":50,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"fifty","entities":[]}}}}}}]}}"#,
            extra.0
        )
    };
    apply_json(&mut session, &seq, &sink, &page(first));
    assert!(
        matches!(
            session.search.chat_search.jump,
            ChatSearchJump::Loading { request, .. } if request == again
        ),
        "the first request's page is not the answer to the third jump"
    );
    apply_json(&mut session, &seq, &sink, &page(again));
    assert_eq!(
        session.search.chat_search.jump,
        ChatSearchJump::Ready {
            message_id: MessageId(50)
        }
    );
}
