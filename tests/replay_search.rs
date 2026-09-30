//! Global and in-chat search replay tests.
//! Split from `tests/replay.rs` — pure code motion.
mod replay_common;
use replay_common::*;

#[test]
fn replay_global_search_happy_empty_and_error() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
            r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9","is_pinned":false}}"#,
        ],
    );
    let recents_gen = session.search.begin_recents();
    let recents_extra =
        session.request_search(RequestPurpose::SearchRecentlyFoundChats, recents_gen);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[7]}}"#,
            recents_extra.0
        )],
    );
    assert_eq!(session.search.status, quill::state::SearchStatus::Ready);
    assert!(session.search.recents);
    assert_eq!(session.search.chat_ids[0].0, 7);

    let search_gen = session.search.begin_query("hello");
    let chats_extra = session.request_search(RequestPurpose::SearchChats, search_gen);
    let messages_extra = session.request_search(RequestPurpose::SearchMessages, search_gen);
    let public_extra = session.request_search(RequestPurpose::SearchPublicChats, search_gen);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            &format!(
                r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[7]}}"#,
                chats_extra.0
            ),
            &format!(
                r#"{{"@type":"foundMessages","@extra":"{}","total_count":1,"next_offset":"","messages":[{{"id":50,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"CANARY_REPLAY_search","entities":[]}}}}}}]}}"#,
                messages_extra.0
            ),
            // Phase 7.2 replay: `searchPublicChats` returns an unknown public
            // channel; status waits for it before resolving to Ready.
            &format!(
                r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[4242]}}"#,
                public_extra.0
            ),
        ],
    );
    assert_eq!(session.search.status, quill::state::SearchStatus::Ready);
    assert_eq!(session.search.chat_ids[0].0, 7);
    assert_eq!(session.search.public_chat_ids[0].0, 4242);
    session.promote_search_message(quill::ids::ChatId(7), quill::ids::MessageId(50));
    session.open_chat(quill::ids::ChatId(7));
    assert!(
        session
            .histories
            .get(&7)
            .unwrap()
            .messages
            .contains_key(&50)
    );

    let search_gen = session.search.begin_query("zzz");
    let chats_extra = session.request_search(RequestPurpose::SearchChats, search_gen);
    let messages_extra = session.request_search(RequestPurpose::SearchMessages, search_gen);
    let public_extra = session.request_search(RequestPurpose::SearchPublicChats, search_gen);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            &format!(
                r#"{{"@type":"chats","@extra":"{}","total_count":0,"chat_ids":[]}}"#,
                chats_extra.0
            ),
            &format!(
                r#"{{"@type":"foundMessages","@extra":"{}","total_count":0,"next_offset":"","messages":[]}}"#,
                messages_extra.0
            ),
            &format!(
                r#"{{"@type":"chats","@extra":"{}","total_count":0,"chat_ids":[]}}"#,
                public_extra.0
            ),
        ],
    );
    assert_eq!(session.search.status, quill::state::SearchStatus::Empty);

    let search_gen = session.search.begin_query("nope");
    let chats_extra = session.request_search(RequestPurpose::SearchChats, search_gen);
    let messages_extra = session.request_search(RequestPurpose::SearchMessages, search_gen);
    let public_extra = session.request_search(RequestPurpose::SearchPublicChats, search_gen);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            &format!(
                r#"{{"@type":"error","code":400,"message":"CANARY_REPLAY_search_err","@extra":"{}"}}"#,
                chats_extra.0
            ),
            &format!(
                r#"{{"@type":"error","code":400,"message":"CANARY_REPLAY_search_err2","@extra":"{}"}}"#,
                messages_extra.0
            ),
            &format!(
                r#"{{"@type":"error","code":400,"message":"CANARY_REPLAY_search_err3","@extra":"{}"}}"#,
                public_extra.0
            ),
        ],
    );
    assert_eq!(session.search.status, quill::state::SearchStatus::Failed);
    session.close_search();
    assert_eq!(session.search.status, quill::state::SearchStatus::Closed);
    assert!(!sink.rendered().contains("CANARY_REPLAY"));
}

#[test]
fn replay_chat_search_generation_jump_and_empty() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
            r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9","is_pinned":false}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":50,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hello already loaded","entities":[]}}}}"#,
        ],
    );
    session.open_chat(quill::ids::ChatId(7));
    assert!(session.open_chat_search());
    let search_gen = session.chat_search.begin_query("hello");
    let extra = session.request_chat_search(
        RequestPurpose::SearchChatMessages,
        quill::ids::ChatId(7),
        search_gen,
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":2,"next_from_message_id":40,"messages":[{{"id":50,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"CANARY_REPLAY_chat","entities":[]}}}}}},{{"id":40,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"older hello","entities":[]}}}}}}]}}"#,
            extra.0
        )],
    );
    assert_eq!(
        session.chat_search.status,
        quill::state::SearchStatus::Ready
    );
    assert_eq!(session.chat_search.hits.len(), 2);
    assert_eq!(
        session.begin_chat_search_jump(quill::ids::MessageId(50)),
        quill::state::ChatSearchJumpNeed::AlreadyReady
    );
    assert_eq!(
        session.begin_chat_search_jump(quill::ids::MessageId(40)),
        quill::state::ChatSearchJumpNeed::LoadAround
    );
    let around = session.request_history_around(quill::ids::ChatId(7), quill::ids::MessageId(40));
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":2,"messages":[{{"id":40,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"older hello","entities":[]}}}}}},{{"id":39,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"neighbor","entities":[]}}}}}}]}}"#,
            around.0
        )],
    );
    assert_eq!(
        session.chat_search.jump,
        quill::state::ChatSearchJump::Ready {
            message_id: quill::ids::MessageId(40)
        }
    );
    assert!(
        session
            .histories
            .get(&7)
            .unwrap()
            .contains(quill::ids::MessageId(39))
    );

    let stale = session.chat_search.begin_query("old");
    let stale_extra = session.request_chat_search(
        RequestPurpose::SearchChatMessages,
        quill::ids::ChatId(7),
        stale,
    );
    let fresh = session.chat_search.begin_query("new");
    let _fresh_extra = session.request_chat_search(
        RequestPurpose::SearchChatMessages,
        quill::ids::ChatId(7),
        fresh,
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":1,"next_from_message_id":0,"messages":[{{"id":1,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"stale","entities":[]}}}}}}]}}"#,
            stale_extra.0
        )],
    );
    assert_eq!(
        session.chat_search.status,
        quill::state::SearchStatus::Searching
    );
    assert!(session.chat_search.hits.is_empty());

    let empty_gen = session.chat_search.begin_query("zzz");
    let empty_extra = session.request_chat_search(
        RequestPurpose::SearchChatMessages,
        quill::ids::ChatId(7),
        empty_gen,
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[&format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":0,"next_from_message_id":0,"messages":[]}}"#,
            empty_extra.0
        )],
    );
    assert_eq!(
        session.chat_search.status,
        quill::state::SearchStatus::Empty
    );

    let open_chat = session.open_chat;
    session.close_chat_search();
    assert_eq!(
        session.chat_search.status,
        quill::state::SearchStatus::Closed
    );
    assert_eq!(session.open_chat, open_chat);
    assert!(
        session
            .histories
            .get(&7)
            .unwrap()
            .contains(quill::ids::MessageId(50))
    );
    assert!(!sink.rendered().contains("CANARY_REPLAY"));
}
