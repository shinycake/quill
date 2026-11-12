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
    session.search.search.begin_query("al");
    let chats = session.request_search(RequestPurpose::SearchChats);
    let messages = session.request_search(RequestPurpose::SearchMessages);
    let public = session.request_search(RequestPurpose::SearchPublicChats);
    let server = session.request_search(RequestPurpose::SearchChatsOnServer);
    // The three gating answers settle the search ...
    apply_json(&mut session, &seq, &sink, &chats_json(chats, "[11]"));
    apply_json(&mut session, &seq, &sink, &chats_json(public, "[11,42]"));
    apply_json(&mut session, &seq, &sink, &no_messages_json(messages));
    assert_eq!(session.search.search.status(), SearchStatus::Ready);
    // ... and the late server answer adds only what was missing.
    apply_json(&mut session, &seq, &sink, &chats_json(server, "[11,77]"));
    assert_eq!(
        session.search.search.merged_chat_ids(),
        vec![ChatId(11), ChatId(77)]
    );
    assert_eq!(
        session.search.search.public_only_chat_ids(),
        vec![ChatId(42)]
    );
}

#[test]
fn server_chats_alone_turn_empty_into_ready() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_search();
    session.search.search.begin_query("zed");
    let chats = session.request_search(RequestPurpose::SearchChats);
    let messages = session.request_search(RequestPurpose::SearchMessages);
    let public = session.request_search(RequestPurpose::SearchPublicChats);
    let server = session.request_search(RequestPurpose::SearchChatsOnServer);
    apply_json(&mut session, &seq, &sink, &chats_json(chats, "[]"));
    apply_json(&mut session, &seq, &sink, &chats_json(public, "[]"));
    apply_json(&mut session, &seq, &sink, &no_messages_json(messages));
    assert_eq!(session.search.search.status(), SearchStatus::Empty);
    apply_json(&mut session, &seq, &sink, &chats_json(server, "[5]"));
    assert_eq!(session.search.search.status(), SearchStatus::Ready);
}

#[test]
fn a_failed_server_search_changes_nothing() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_search();
    session.search.search.begin_query("al");
    let _chats = session.request_search(RequestPurpose::SearchChats);
    let server = session.request_search(RequestPurpose::SearchChatsOnServer);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"boom"}}"#,
            server.0
        ),
    );
    assert!(session.search.search.server_chats.items().is_empty());
    assert_eq!(session.search.search.status(), SearchStatus::Searching);
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
    session.search.search.begin_query("dune");
    session.search.search.begin_public_scope();
    let extra = session.request_search(RequestPurpose::SearchPublicPosts);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &public_post_json(extra, false, true),
    );
    assert_eq!(session.search.search.status(), SearchStatus::Ready);
    assert_eq!(session.search.search.messages.len(), 1);
    assert!(!session.search.search.public_limits_exceeded());

    session.search.search.begin_query("arrakis");
    session.search.search.begin_public_scope();
    let extra = session.request_search(RequestPurpose::SearchPublicPosts);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &public_post_json(extra, true, false),
    );
    assert!(session.search.search.public_limits_exceeded());
    assert_eq!(session.search.search.status(), SearchStatus::Empty);
}

#[test]
fn tag_search_results_land_in_the_messages() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_search();
    session.search.search.begin_query("#dune");
    session.search.search.begin_public_scope();
    let extra = session.request_search(RequestPurpose::SearchPublicMessagesByTag);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r##"{{"@type":"foundMessages","@extra":"{}","total_count":1,"next_offset":"","messages":[{{"id":9,"chat_id":-100,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"#dune","entities":[]}}}}}}]}}"##,
            extra.0
        ),
    );
    assert_eq!(session.search.search.status(), SearchStatus::Ready);
    assert_eq!(session.search.search.messages.len(), 1);
}

#[test]
fn frequent_contacts_load_and_the_option_hides_them() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetTopChats, None);
    apply_json(&mut session, &seq, &sink, &chats_json(extra, "[11,12]"));
    assert_eq!(
        session.search.search.top_chats,
        vec![ChatId(11), ChatId(12)]
    );
    assert!(session.search.search.remove_top_chat(ChatId(11)));
    assert_eq!(session.search.search.top_chats, vec![ChatId(12)]);
    assert!(!session.search.search.remove_top_chat(ChatId(11)));

    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateOption","name":"disable_top_chats","value":{"@type":"optionValueBoolean","value":true}}"#,
    );
    assert!(session.search.search.top_chats_disabled);
    assert!(session.search.search.top_chats.is_empty());
    // A reply that was already in flight must not bring the strip back.
    let late = session.request(RequestPurpose::GetTopChats, None);
    apply_json(&mut session, &seq, &sink, &chats_json(late, "[12]"));
    assert!(session.search.search.top_chats.is_empty());
}

#[test]
fn removing_one_recent_keeps_the_rest_and_idles_when_empty() {
    let mut search = SearchState {
        surface: SearchSurface::Recents,
        chat_ids: vec![ChatId(1), ChatId(2)],
        ..SearchState::default()
    };
    assert!(search.remove_recent(ChatId(1)));
    assert_eq!(search.chat_ids, vec![ChatId(2)]);
    assert_eq!(search.status(), SearchStatus::Ready);
    assert!(!search.remove_recent(ChatId(1)));
    assert!(search.remove_recent(ChatId(2)));
    assert_eq!(search.status(), SearchStatus::Idle);
    // Typed results are not "recents": nothing to remove.
    search.surface = SearchSurface::Query;
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
            .last()
            .is_some_and(|note| note.contains("recent search"))
    );
}

#[test]
fn top_people_lists_known_chats_by_first_name_and_hides_when_disabled() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    for json in [
        r#"{"@type":"updateUser","user":{"id":31,"first_name":"Ada","last_name":"Lovelace","type":{"@type":"userTypeRegular"},"status":{"@type":"userStatusOnline","expires":4102444800}}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":31,"title":"Ada Lovelace","type":{"@type":"chatTypePrivate","user_id":31},"unread_count":4}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":32,"title":"Grace Hopper","type":{"@type":"chatTypePrivate","user_id":32},"unread_count":0,"is_marked_as_unread":true}}"#,
    ] {
        apply_json(&mut session, &seq, &sink, json);
    }
    // 99 is not a chat the account knows: tdesktop skips inaccessible users.
    session.search.search.top_chats = vec![ChatId(31), ChatId(99), ChatId(32)];
    let people = crate::top_people::top_people(&session);
    let names: Vec<&str> = people.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["Ada", "Grace"]);
    assert_eq!(people.first().and_then(|p| p.unread()), Some((4, false)));
    assert_eq!(people.get(1).and_then(|p| p.unread()), Some((0, true)));
    assert!(people.first().is_some_and(|p| p.online()));
    assert!(people.iter().all(|p| !p.muted()));
    session.search.search.top_chats_disabled = true;
    assert!(crate::top_people::top_people(&session).is_empty());
}

#[test]
fn a_leg_settles_only_on_the_request_it_waits_for() {
    // Two `searchChats` asks inside one query: the second replaces the
    // leg's request, so the first one's late answer settles nothing.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_search();
    session.search.search.begin_query("al");
    let first = session.request_search(RequestPurpose::SearchChats);
    let second = session.request_search(RequestPurpose::SearchChats);
    apply_json(&mut session, &seq, &sink, &chats_json(first, "[11]"));
    assert!(session.search.search.chat_ids.is_empty());
    assert_eq!(session.search.search.status(), SearchStatus::Searching);
    apply_json(&mut session, &seq, &sink, &chats_json(second, "[42]"));
    assert_eq!(session.search.search.chat_ids, vec![ChatId(42)]);
}

#[test]
fn a_page_of_a_closed_list_is_dropped_and_the_next_page_waits_for_its_own_request() {
    use crate::state::SearchPaging;
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.open_search();
    session.search.search.begin_query("dune");
    session.search.search.set_first_page_offset("p2".into());
    let first = session.request_search(RequestPurpose::SearchMessagesMore);
    session.search.search.tabs.page_sent(first);
    // The query changes: the list starts over and the page is no longer out.
    session.search.search.begin_query("arrakis");
    session.search.search.set_first_page_offset("q2".into());
    assert_eq!(session.search.search.tabs.paging, SearchPaging::Idle);
    let second = session.request_search(RequestPurpose::SearchMessagesMore);
    session.search.search.tabs.page_sent(second);
    apply_json(&mut session, &seq, &sink, &no_messages_json(first));
    assert_eq!(
        session.search.search.tabs.paging,
        SearchPaging::Loading { request: second },
        "the old page's answer does not end the new page's wait"
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"boom"}}"#,
            first.0
        ),
    );
    assert!(session.search.search.tabs.loading_more());
    apply_json(&mut session, &seq, &sink, &no_messages_json(second));
    assert_eq!(session.search.search.tabs.paging, SearchPaging::Idle);
}
