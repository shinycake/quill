//! State reducer tests: search upgrades (server chats, public posts,
//! frequent contacts, removing recents).
use super::common::*;
use super::*;

fn chats_json(extra: RequestId, ids: &str) -> String {
    format!(
        r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":{ids}}}"#,
        extra.0
    )
}

fn no_messages_json(extra: RequestId) -> String {
    format!(
        r#"{{"@type":"foundMessages","@extra":"{}","total_count":0,"next_offset":"","messages":[]}}"#,
        extra.0
    )
}

#[test]
fn server_chats_merge_behind_local_hits_without_gating_status() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_search();
    let search_gen = session.search.begin_query("al");
    let chats = session.request_search(RequestPurpose::SearchChats, search_gen);
    let messages = session.request_search(RequestPurpose::SearchMessages, search_gen);
    let public = session.request_search(RequestPurpose::SearchPublicChats, search_gen);
    let server = session.request_search(RequestPurpose::SearchChatsOnServer, search_gen);
    // The three gating answers settle the search ...
    apply_json(&mut session, &seq, &sink, &chats_json(chats, "[11]"));
    apply_json(&mut session, &seq, &sink, &chats_json(public, "[11,42]"));
    apply_json(&mut session, &seq, &sink, &no_messages_json(messages));
    assert_eq!(session.search.status, SearchStatus::Ready);
    // ... and the late server answer adds only what was missing.
    apply_json(&mut session, &seq, &sink, &chats_json(server, "[11,77]"));
    assert_eq!(
        session.search.merged_chat_ids(),
        vec![ChatId(11), ChatId(77)]
    );
    assert_eq!(session.search.public_only_chat_ids(), vec![ChatId(42)]);
}

#[test]
fn server_chats_alone_turn_empty_into_ready() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_search();
    let search_gen = session.search.begin_query("zed");
    let chats = session.request_search(RequestPurpose::SearchChats, search_gen);
    let messages = session.request_search(RequestPurpose::SearchMessages, search_gen);
    let public = session.request_search(RequestPurpose::SearchPublicChats, search_gen);
    let server = session.request_search(RequestPurpose::SearchChatsOnServer, search_gen);
    apply_json(&mut session, &seq, &sink, &chats_json(chats, "[]"));
    apply_json(&mut session, &seq, &sink, &chats_json(public, "[]"));
    apply_json(&mut session, &seq, &sink, &no_messages_json(messages));
    assert_eq!(session.search.status, SearchStatus::Empty);
    apply_json(&mut session, &seq, &sink, &chats_json(server, "[5]"));
    assert_eq!(session.search.status, SearchStatus::Ready);
}

#[test]
fn a_failed_server_search_changes_nothing() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_search();
    let search_gen = session.search.begin_query("al");
    let server = session.request_search(RequestPurpose::SearchChatsOnServer, search_gen);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"boom"}}"#,
            server.0
        ),
    );
    assert!(session.search.server_chat_ids.is_empty());
    assert_eq!(session.search.status, SearchStatus::Searching);
}

fn public_post_json(extra: RequestId, exceeded: bool, with_message: bool) -> String {
    let message = if with_message {
        r#"{"id":9,"chat_id":-100,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Dune review","entities":[]}}}"#
    } else {
        ""
    };
    format!(
        r#"{{"@type":"foundPublicPosts","@extra":"{}","next_offset":"","are_limits_exceeded":{exceeded},"messages":[{message}]}}"#,
        extra.0
    )
}

#[test]
fn public_posts_fill_the_messages_and_flag_an_exhausted_quota() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_search();
    let search_gen = session.search.begin_query("dune");
    session.search.begin_public_scope();
    let extra = session.request_search(RequestPurpose::SearchPublicPosts, search_gen);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &public_post_json(extra, false, true),
    );
    assert_eq!(session.search.status, SearchStatus::Ready);
    assert_eq!(session.search.messages.len(), 1);
    assert!(!session.search.public_limits_exceeded);

    let search_gen = session.search.begin_query("arrakis");
    session.search.begin_public_scope();
    let extra = session.request_search(RequestPurpose::SearchPublicPosts, search_gen);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &public_post_json(extra, true, false),
    );
    assert!(session.search.public_limits_exceeded);
    assert_eq!(session.search.status, SearchStatus::Empty);
}

#[test]
fn tag_search_results_land_in_the_messages() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_search();
    let search_gen = session.search.begin_query("#dune");
    session.search.begin_public_scope();
    let extra = session.request_search(RequestPurpose::SearchPublicMessagesByTag, search_gen);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r##"{{"@type":"foundMessages","@extra":"{}","total_count":1,"next_offset":"","messages":[{{"id":9,"chat_id":-100,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"#dune","entities":[]}}}}}}]}}"##,
            extra.0
        ),
    );
    assert_eq!(session.search.status, SearchStatus::Ready);
    assert_eq!(session.search.messages.len(), 1);
}

#[test]
fn frequent_contacts_load_and_the_option_hides_them() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetTopChats, None);
    apply_json(&mut session, &seq, &sink, &chats_json(extra, "[11,12]"));
    assert_eq!(session.search.top_chats, vec![ChatId(11), ChatId(12)]);
    assert!(session.search.remove_top_chat(ChatId(11)));
    assert_eq!(session.search.top_chats, vec![ChatId(12)]);
    assert!(!session.search.remove_top_chat(ChatId(11)));

    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateOption","name":"disable_top_chats","value":{"@type":"optionValueBoolean","value":true}}"#,
    );
    assert!(session.search.top_chats_disabled);
    assert!(session.search.top_chats.is_empty());
    // A reply that was already in flight must not bring the strip back.
    let late = session.request(RequestPurpose::GetTopChats, None);
    apply_json(&mut session, &seq, &sink, &chats_json(late, "[12]"));
    assert!(session.search.top_chats.is_empty());
}

#[test]
fn removing_one_recent_keeps_the_rest_and_idles_when_empty() {
    let mut search = SearchState {
        recents: true,
        chat_ids: vec![ChatId(1), ChatId(2)],
        status: SearchStatus::Ready,
        ..SearchState::default()
    };
    assert!(search.remove_recent(ChatId(1)));
    assert_eq!(search.chat_ids, vec![ChatId(2)]);
    assert_eq!(search.status, SearchStatus::Ready);
    assert!(!search.remove_recent(ChatId(1)));
    assert!(search.remove_recent(ChatId(2)));
    assert_eq!(search.status, SearchStatus::Idle);
    // Typed results are not "recents": nothing to remove.
    search.recents = false;
    search.chat_ids = vec![ChatId(3)];
    assert!(!search.remove_recent(ChatId(3)));
}

#[test]
fn a_refused_remove_surfaces_a_note() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::RemoveRecentlyFoundChat, Some(ChatId(5)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"no"}}"#,
            extra.0
        ),
    );
    assert!(
        session
            .chats_state
            .chat_action_error
            .as_deref()
            .is_some_and(|note| note.contains("recent search"))
    );
}
