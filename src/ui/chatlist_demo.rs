//! Screenshot fixtures for the archive row, story rings and the pinned
//! drag: everything is injected through the normal reducer (no live
//! Telegram).

use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
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

const DUROV_CHANNEL: i64 = -1001006503122;
const CODE_CHANNEL: i64 = -1001900000001;
const MULTILINE_CHAT: i64 = 41;

/// `ReadyJoinBar` fixture: a public channel the viewer never joined, opened
/// from search or a `t.me` link. TDLib sends `updateSupergroup` (own status
/// `chatMemberStatusLeft`) before `updateNewChat`, and `getChatMember(me)` is
/// never answered here. Before the fix the bar read "Checking channel
/// membership…" forever; it must read "Join channel".
pub(super) fn apply_ready_join_bar(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let jsons = [
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":1006503122,"usernames":{"@type":"usernames","active_usernames":["durov"],"disabled_usernames":[],"editable_username":"durov"},"status":{"@type":"chatMemberStatusLeft"},"member_count":0,"is_channel":true,"is_broadcast_group":false}}"#.to_string(),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{DUROV_CHANNEL},"title":"Durov's Channel","type":{{"@type":"chatTypeSupergroup","supergroup_id":1006503122,"is_channel":true}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":1048576,"chat_id":{DUROV_CHANNEL},"sender_id":{{"@type":"messageSenderChat","chat_id":{DUROV_CHANNEL}}},"is_outgoing":false,"is_channel_post":true,"date":1790632300,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"A post from a public channel you have not joined.","entities":[]}}}}}}}}"#
        ),
    ];
    session.open_chat(ChatId(DUROV_CHANNEL));
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// Preview-row fixtures: a private chat whose last message has a hard
/// newline and a public channel whose last message starts with a custom
/// emoji and continues on a second line. `listed` puts both in the main chat
/// list; otherwise they only exist for the search results.
fn apply_preview_chats(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
    listed: bool,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let position = |order: i64| {
        if listed {
            format!(
                r#","positions":[{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{order}","is_pinned":false}}]"#
            )
        } else {
            String::new()
        }
    };
    let jsons = [
        r#"{"@type":"updateUser","user":{"@type":"user","id":41,"first_name":"Tali","last_name":"Rosen","usernames":null,"phone_number":"","status":{"@type":"userStatusRecently"},"profile_photo":null,"is_contact":true,"type":{"@type":"userTypeRegular"}}}"#.to_string(),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{MULTILINE_CHAT},"title":"Tali Rosen","type":{{"@type":"chatTypePrivate","user_id":41}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatLastMessage","chat_id":{MULTILINE_CHAT},"last_message":{{"id":2097152,"chat_id":{MULTILINE_CHAT},"date":1790632300,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Shopping list:\nmilk\neggs\nbread","entities":[]}}}}}}{}}}"#,
            position(5000)
        ),
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":1900000001,"usernames":{"@type":"usernames","active_usernames":["durovscode"],"disabled_usernames":[],"editable_username":"durovscode"},"status":{"@type":"chatMemberStatusLeft"},"member_count":0,"is_channel":true,"is_broadcast_group":false}}"#.to_string(),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{CODE_CHANNEL},"title":"Durov's Code","type":{{"@type":"chatTypeSupergroup","supergroup_id":1900000001,"is_channel":true}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatLastMessage","chat_id":{CODE_CHANNEL},"last_message":{{"id":3145728,"chat_id":{CODE_CHANNEL},"date":1790632200,"is_outgoing":false,"is_channel_post":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"🫠 Galaxy, we have a problem\nSamsung phones now ship with a pre-installed app that cannot be removed","entities":[{{"@type":"textEntity","offset":0,"length":2,"type":{{"@type":"textEntityTypeCustomEmoji","custom_emoji_id":"4242"}}}}]}}}}}}{}}}"#,
            position(4900)
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// `ReadySearchPreviews` fixture: chat-list search results with the
/// multi-line and custom-emoji previews, in "Chats" and "Public chats".
pub(super) fn apply_ready_search_previews(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    apply_preview_chats(session, sink, seq, false);
    session.open_search();
    session.search.begin_query("durov");
    session.search.chat_ids = vec![ChatId(MULTILINE_CHAT)];
    session.search.public_chat_ids = vec![ChatId(CODE_CHANNEL)];
    session.search.status = quill::state::SearchStatus::Ready;
}

/// `ReadyMultilineRows` fixture: the same two chats as normal chat-list rows.
pub(super) fn apply_ready_multiline_rows(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    apply_preview_chats(session, sink, seq, true);
}
