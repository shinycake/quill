//! Chat-list archive row and story-ring state plumbing through the real
//! reducer: the archive summary (names, unread badge) and the per-chat
//! story ring (kept for every story list, unread by `max_read_story_id`).

use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::AccountKey;
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn session() -> (Session, Arc<dyn DiagnosticSink>, AtomicU64) {
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::default());
    (
        Session::new(AccountKey::primary(), sink.clone()),
        sink,
        AtomicU64::new(0),
    )
}

fn apply(s: &mut Session, sink: &Arc<dyn DiagnosticSink>, seq: &AtomicU64, json: &str) {
    s.apply(copy_and_parse(json, seq, sink).unwrap());
}

fn archived_chat(
    s: &mut Session,
    sink: &Arc<dyn DiagnosticSink>,
    seq: &AtomicU64,
    id: i64,
    title: &str,
    unread: i32,
    date: i64,
) {
    apply(
        s,
        sink,
        seq,
        &format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{{"@type":"chatTypePrivate","user_id":{id}}},"unread_count":{unread}}}}}"#
        ),
    );
    apply(
        s,
        sink,
        seq,
        &format!(
            r#"{{"@type":"updateChatLastMessage","chat_id":{id},"last_message":{{"id":{id}1,"chat_id":{id},"is_outgoing":false,"date":{date},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"x","entities":[]}}}}}},"positions":[{{"@type":"chatPosition","list":{{"@type":"chatListArchive"}},"order":"{id}","is_pinned":false}}]}}"#
        ),
    );
}

#[test]
fn archive_summary_names_newest_chats_and_counts_unread_chats() {
    let (mut s, sink, seq) = session();
    assert!(s.archive_row_summary().is_none());
    archived_chat(&mut s, &sink, &seq, 1, "Old", 0, 100);
    archived_chat(&mut s, &sink, &seq, 2, "Newest", 5, 300);
    archived_chat(&mut s, &sink, &seq, 3, "Middle", 1, 200);
    let summary = s.archive_row_summary().unwrap();
    assert_eq!(summary.text(), "Newest, Middle, Old");
    // The badge counts chats, not messages (6 unread messages, 2 chats).
    assert_eq!(summary.badge().as_deref(), Some("2"));
    assert!(summary.names[0].unread && !summary.names[2].unread);
}

#[test]
fn story_ring_follows_active_stories_in_any_list() {
    let (mut s, sink, seq) = session();
    let stories = |list: &str, ids: &str, max_read: i32| {
        format!(
            r#"{{"@type":"updateChatActiveStories","active_stories":{{"@type":"chatActiveStories","chat_id":7,"list":{list},"order":"1","can_be_archived":false,"max_read_story_id":{max_read},"stories":[{ids}]}}}}"#
        )
    };
    let info = |id: i32| {
        format!(
            r#"{{"@type":"storyInfo","story_id":{id},"date":1,"is_for_close_friends":false,"is_live":false}}"#
        )
    };
    assert!(s.chat_story_ring(7).is_none());
    let ids = format!("{},{},{}", info(1), info(2), info(3));
    apply(
        &mut s,
        &sink,
        &seq,
        &stories(r#"{"@type":"storyListMain"}"#, &ids, 1),
    );
    let ring = s.chat_story_ring(7).unwrap();
    assert_eq!((ring.count, ring.unread), (3, 2));
    // Everything read: still a ring, now grey.
    apply(
        &mut s,
        &sink,
        &seq,
        &stories(r#"{"@type":"storyListMain"}"#, &ids, 3),
    );
    assert!(!s.chat_story_ring(7).unwrap().has_unread());
    // Archived story list: no tray entry, but the avatar keeps its ring.
    apply(
        &mut s,
        &sink,
        &seq,
        &stories(r#"{"@type":"storyListArchive"}"#, &ids, 0),
    );
    assert!(s.chat_story_ring(7).is_some());
    assert!(s.ordered_story_tray().is_empty());
    // Stories expire: the ring goes.
    apply(
        &mut s,
        &sink,
        &seq,
        &stories(r#"{"@type":"storyListMain"}"#, "", 0),
    );
    assert!(s.chat_story_ring(7).is_none());
}
