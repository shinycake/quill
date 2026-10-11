//! Parity slice `parity:communities-chatlist-mode`: community chat-list
//! mode filter logic, through the real `Session` reducer path —
//! `updateNewChat` / `updateChatPosition` build the chat list,
//! `updateCommunityFullInfo` builds the membership pack, and
//! `retain_community_chats` (the production predicate, called from the
//! UI at the same shape) narrows it.

use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use quill::community_mode::{community_member_ids, retain_community_chats};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::AccountKey;
use quill::state::{ChatSummary, Session};
use quill::telegram::client::copy_and_parse;

fn feed(session: &mut Session, seq: &AtomicU64, sink: &Arc<dyn DiagnosticSink>, json: &str) {
    let owned = copy_and_parse(json, seq, sink).expect("test json parses");
    session.apply(owned);
}

fn new_chat(
    session: &mut Session,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
    id: i64,
    order: &str,
) {
    feed(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"chat {id}","type":{{"@type":"chatTypePrivate","user_id":{id}}},"unread_count":0}}}}"#
        ),
    );
    feed(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"updateChatPosition","chat_id":{id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{order}","is_pinned":false}}}}"#
        ),
    );
}

fn full_info(
    session: &mut Session,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
    chats_json: &str,
) {
    feed(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"updateCommunityFullInfo","community_id":42,"community_full_info":{{"@type":"communityFullInfo","chats":[{chats_json}],"administrator_count":1,"banned_count":0,"add_chat_request_count":0}}}}"#
        ),
    );
}

/// Three main-list chats (11, 13, 99); community 42's pack holds 11 and
/// 13, with 13 hidden.
fn session_with_community() -> Session {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let seq = AtomicU64::new(0);
    let mut session = Session::new(AccountKey::primary(), dyn_sink.clone());
    new_chat(&mut session, &seq, &dyn_sink, 11, "30");
    new_chat(&mut session, &seq, &dyn_sink, 13, "20");
    new_chat(&mut session, &seq, &dyn_sink, 99, "10");
    full_info(
        &mut session,
        &seq,
        &dyn_sink,
        r#"{"@type":"communityChat","chat_id":11,"can_view_history":true,"is_hidden":false},{"@type":"communityChat","chat_id":13,"can_view_history":true,"is_hidden":true}"#,
    );
    session
}

fn filtered_ids(session: &Session, community_id: Option<i64>) -> Vec<i64> {
    let mut chats: Vec<ChatSummary> = session.ordered_chats().into_iter().cloned().collect();
    retain_community_chats(
        &mut chats,
        community_id,
        &session.groups.community_full_infos,
    );
    chats.iter().map(|c| c.id.0).collect()
}

#[test]
fn community_filter_keeps_only_member_chats() {
    let session = session_with_community();
    assert_eq!(filtered_ids(&session, Some(42)), vec![11, 13]);
}

#[test]
fn community_filter_includes_hidden_member_chats() {
    // `is_hidden` ("visible only to community administrators") does not
    // drop the chat: the mode only narrows the already-visible main
    // list, and the hub lists owned communities.
    let session = session_with_community();
    let members = community_member_ids(session.groups.community_full_infos.get(&42));
    assert!(members.contains(&13));
    assert!(filtered_ids(&session, Some(42)).contains(&13));
}

#[test]
fn community_filter_empty_pack_is_empty_state_not_crash() {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let seq = AtomicU64::new(0);
    let mut session = Session::new(AccountKey::primary(), dyn_sink.clone());
    new_chat(&mut session, &seq, &dyn_sink, 11, "30");
    full_info(&mut session, &seq, &dyn_sink, "");
    assert!(filtered_ids(&session, Some(42)).is_empty());
}

#[test]
fn community_filter_missing_pack_keeps_nothing() {
    let session = session_with_community();
    // Community 43 was never fetched: nothing matches (the UI renders
    // "Loading…" for this case).
    assert!(filtered_ids(&session, Some(43)).is_empty());
}

#[test]
fn community_filter_clear_keeps_everything() {
    let session = session_with_community();
    // `None` = mode cleared / never entered.
    assert_eq!(filtered_ids(&session, None), vec![11, 13, 99]);
}

#[test]
fn community_filter_follows_membership_changes() {
    // A chat leaving the community (pack replaced by a fresh
    // `updateCommunityFullInfo`) drops out of the filtered list.
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let seq = AtomicU64::new(0);
    let mut session = Session::new(AccountKey::primary(), dyn_sink.clone());
    new_chat(&mut session, &seq, &dyn_sink, 11, "30");
    new_chat(&mut session, &seq, &dyn_sink, 13, "20");
    full_info(
        &mut session,
        &seq,
        &dyn_sink,
        r#"{"@type":"communityChat","chat_id":11,"can_view_history":true,"is_hidden":false},{"@type":"communityChat","chat_id":13,"can_view_history":true,"is_hidden":false}"#,
    );
    assert_eq!(filtered_ids(&session, Some(42)), vec![11, 13]);
    full_info(
        &mut session,
        &seq,
        &dyn_sink,
        r#"{"@type":"communityChat","chat_id":11,"can_view_history":true,"is_hidden":false}"#,
    );
    assert_eq!(filtered_ids(&session, Some(42)), vec![11]);
}

#[test]
fn community_member_ids_none_pack_is_empty() {
    assert!(community_member_ids(None).is_empty());
}
