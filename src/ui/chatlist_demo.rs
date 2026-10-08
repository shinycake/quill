//! Screenshot fixtures for the archive row, story rings and the pinned
//! drag: everything is injected through the normal reducer (no live
//! Telegram).

use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

/// Archived chats (two with unread messages), three pinned main chats
/// and story rings on chats 11 (two unread of three), 12 (all read) and
/// 13 (one unread) — on top of the `ReadyChats` seed.
pub(super) fn apply_ready_archive_row(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let archived = |id: i64, title: &str, unread: i32, date: i64, order: i64| {
        [
            format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{{"@type":"chatTypePrivate","user_id":{id}}},"unread_count":{unread},"last_read_inbox_message_id":0,"last_read_outbox_message_id":0}}}}"#
            ),
            format!(
                r#"{{"@type":"updateChatLastMessage","chat_id":{id},"last_message":{{"id":{id}1,"chat_id":{id},"is_outgoing":false,"date":{date},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Archived hello","entities":[]}}}}}},"positions":[{{"@type":"chatPosition","list":{{"@type":"chatListArchive"}},"order":"{order}","is_pinned":false}}]}}"#
            ),
        ]
    };
    let mut jsons: Vec<String> = Vec::new();
    jsons.extend(archived(31, "Maya Chen", 3, 1790632400, 60));
    jsons.extend(archived(32, "Design crit", 0, 1790632300, 50));
    jsons.extend(archived(33, "Leo Park", 1, 1790632200, 40));
    jsons.extend(archived(34, "Old project", 0, 1790632100, 30));
    // Pin the three main chats.
    for (id, order) in [(11, 30), (12, 29), (13, 28)] {
        jsons.push(format!(
            r#"{{"@type":"updateChatPosition","chat_id":{id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{order}","is_pinned":true}}}}"#
        ));
    }
    // Story rings: chat 11 has stories 5-7 with 5 read (two unread arcs and
    // one read), chat 12 one read story, chat 13 one unread story.
    let stories = |chat_id: i64, ids: &[i32], max_read: i32| {
        let infos = ids
            .iter()
            .map(|id| {
                format!(
                    r#"{{"@type":"storyInfo","story_id":{id},"date":1700000000,"is_for_close_friends":false,"is_live":false}}"#
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            r#"{{"@type":"updateChatActiveStories","active_stories":{{"@type":"chatActiveStories","chat_id":{chat_id},"list":{{"@type":"storyListMain"}},"order":"{chat_id}","can_be_archived":false,"max_read_story_id":{max_read},"stories":[{infos}]}}}}"#
        )
    };
    jsons.push(stories(11, &[5, 6, 7], 5));
    jsons.push(stories(12, &[3], 3));
    jsons.push(stories(13, &[1], 0));
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}
