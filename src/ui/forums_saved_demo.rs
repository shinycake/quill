//! `ready-forums-saved` screenshot demo: Saved Messages grouped by the chats
//! the messages came from, one sublist, a tag filter with named tags, and the
//! forum topic editor with its icon picker — injected through the normal
//! reducer, no live Telegram. `QUILL_DEMO_FORUMS_SAVED_VIEW=sublists|sublist|tag|editor`
//! (default `sublists`). English fixtures only.

use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::{RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::ReactionType;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const ME: i64 = 9;
const NEWS: i64 = 301;
const DANA: i64 = 302;
const DESIGN: i64 = 303;
/// The sublist `sublist` and `tag` open.
const OPEN_SUBLIST: i64 = NEWS;
const FORUM: i64 = 16;

fn text(t: &str) -> String {
    format!(
        r#"{{"@type":"messageText","text":{{"@type":"formattedText","text":{},"entities":[]}}}}"#,
        serde_json::to_string(t).unwrap_or_default()
    )
}

fn saved_message(id: i64, date: i64, body: &str, reactions: &str) -> String {
    format!(
        r#"{{"id":{id},"chat_id":{ME},"sender_id":{{"@type":"messageSenderUser","user_id":{ME}}},"is_outgoing":true,"date":{date},"interaction_info":{{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":{reactions}}},"content":{}}}"#,
        text(body)
    )
}

fn topic(id: i64, kind: &str, pinned: bool, order: i64, date: i64, preview: &str) -> String {
    format!(
        r#"{{"@type":"updateSavedMessagesTopic","topic":{{"@type":"savedMessagesTopic","id":"{id}","type":{kind},"is_pinned":{pinned},"order":"{order}","last_message":{},"draft_message":null}}}}"#,
        saved_message(900 + id, date, preview, "null")
    )
}

fn tags(items: &[(&str, &str, i32)]) -> String {
    let tags: Vec<String> = items
        .iter()
        .map(|(emoji, label, count)| {
            format!(
                r#"{{"tag":{{"@type":"reactionTypeEmoji","emoji":"{emoji}"}},"label":"{label}","count":{count}}}"#
            )
        })
        .collect();
    format!(
        r#"{{"@type":"savedMessagesTags","tags":[{}]}}"#,
        tags.join(",")
    )
}

pub(super) fn apply_ready_forums_saved(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
    view: &str,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let now = quill::local_time::now_unix();
    let chat = |id: i64, title: &str, ty: &str| {
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{ty},"unread_count":0}}}}"#
        )
    };
    let from_chat =
        |id: i64| format!(r#"{{"@type":"savedMessagesTopicTypeSavedFromChat","chat_id":{id}}}"#);
    let mut jsons: Vec<String> = vec![
        format!(
            r#"{{"@type":"updateOption","name":"my_id","value":{{"@type":"optionValueInteger","value":"{ME}"}}}}"#
        ),
        format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{ME},"first_name":"Idan","last_name":"Birman","is_premium":true,"usernames":null,"phone_number":"","status":{{"@type":"userStatusRecently"}},"profile_photo":null,"is_contact":false,"type":{{"@type":"userTypeRegular"}}}}}}"#
        ),
        format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{DANA},"first_name":"Dana","last_name":"Cole","usernames":null,"phone_number":"","status":{{"@type":"userStatusRecently"}},"profile_photo":null,"is_contact":true,"type":{{"@type":"userTypeRegular"}}}}}}"#
        ),
        chat(
            ME,
            "Saved Messages",
            &format!(r#"{{"@type":"chatTypePrivate","user_id":{ME}}}"#),
        ),
        chat(
            NEWS,
            "Rust News",
            &format!(
                r#"{{"@type":"chatTypeSupergroup","supergroup_id":{NEWS},"is_channel":true}}"#
            ),
        ),
        chat(
            DANA,
            "Dana Cole",
            &format!(r#"{{"@type":"chatTypePrivate","user_id":{DANA}}}"#),
        ),
        chat(
            DESIGN,
            "Design Team",
            &format!(r#"{{"@type":"chatTypeBasicGroup","basic_group_id":{DESIGN}}}"#),
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{ME},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"9000","is_pinned":true}}}}"#
        ),
        format!(r#"{{"@type":"updateChatViewAsTopics","chat_id":{ME},"view_as_topics":true}}"#),
        topic(
            NEWS,
            &from_chat(NEWS),
            true,
            90,
            now - 3600,
            "Rust 1.95 is out: let chains everywhere",
        ),
        topic(
            ME,
            r#"{"@type":"savedMessagesTopicTypeMyNotes"}"#,
            true,
            80,
            now - 7200,
            "Buy oat milk and call the dentist",
        ),
        topic(
            DANA,
            &from_chat(DANA),
            false,
            50,
            now - 86_400,
            "The flight lands at 18:40, terminal 3",
        ),
        topic(
            DESIGN,
            &from_chat(DESIGN),
            false,
            40,
            now - 3 * 86_400,
            "Final spacing scale for the sidebar",
        ),
        topic(
            1,
            r#"{"@type":"savedMessagesTopicTypeAuthorHidden"}"#,
            false,
            30,
            now - 9 * 86_400,
            "A quote worth keeping",
        ),
        r#"{"@type":"updateSavedMessagesTopicCount","topic_count":5}"#.to_string(),
        format!(
            r#"{{"@type":"updateSavedMessagesTags","saved_messages_topic_id":"0","tags":{}}}"#,
            tags(&[
                ("\u{2764}", "Read later", 4),
                ("\u{1f525}", "", 2),
                ("\u{1f44d}", "Recipes", 3),
            ])
        ),
    ];
    if view == "editor" {
        jsons.clear();
    }
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    match view {
        "sublist" => open_sublist(session, &dyn_sink, seq, now),
        "tag" => open_tag(session, &dyn_sink, seq, now),
        "editor" => open_editor(session, sink, &dyn_sink, seq),
        _ => session.open_chat(ChatId(ME)),
    }
}

fn open_sublist(session: &mut Session, sink: &Arc<dyn DiagnosticSink>, seq: &AtomicU64, now: i64) {
    session.open_chat(ChatId(ME));
    session.open_saved_sublist(OPEN_SUBLIST);
    let heart = r#"[{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"❤"},"total_count":1,"is_chosen":true,"recent_sender_ids":[]}]"#;
    let page = session.request(
        RequestPurpose::GetSavedMessagesTopicHistory {
            topic_id: OPEN_SUBLIST,
        },
        None,
    );
    let messages = [
        saved_message(
            710,
            now - 3600,
            "Rust 1.95 is out: let chains everywhere",
            heart,
        ),
        saved_message(
            709,
            now - 7200,
            "Cargo now caches build scripts across workspaces",
            "null",
        ),
        saved_message(
            708,
            now - 90_000,
            "Async closures are stable. Time to delete a few boxes.",
            heart,
        ),
    ];
    let json = format!(
        r#"{{"@type":"messages","@extra":"{}","total_count":3,"messages":[{}]}}"#,
        page.0,
        messages.join(",")
    );
    if let Some(owned) = copy_and_parse(&json, seq, sink) {
        session.apply(owned);
    }
    let extra = session.request(
        RequestPurpose::GetSavedMessagesTags {
            topic_id: OPEN_SUBLIST,
        },
        None,
    );
    let json = format!(
        r#"{{"@type":"savedMessagesTags","@extra":"{}","tags":[{{"tag":{{"@type":"reactionTypeEmoji","emoji":"❤"}},"label":"","count":2}},{{"tag":{{"@type":"reactionTypeEmoji","emoji":"🔥"}},"label":"","count":1}}]}}"#,
        extra.0
    );
    if let Some(owned) = copy_and_parse(&json, seq, sink) {
        session.apply(owned);
    }
}

fn open_tag(session: &mut Session, sink: &Arc<dyn DiagnosticSink>, seq: &AtomicU64, now: i64) {
    session.open_chat(ChatId(ME));
    session.begin_saved_tag_search(0, ReactionType::emoji("\u{2764}"));
    let page = session.request(RequestPurpose::SearchSavedMessages { topic_id: 0 }, None);
    let heart = r#"[{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"❤"},"total_count":1,"is_chosen":true,"recent_sender_ids":[]}]"#;
    let messages = [
        saved_message(
            710,
            now - 3600,
            "Rust 1.95 is out: let chains everywhere",
            heart,
        ),
        saved_message(
            640,
            now - 2 * 86_400,
            "Slow-roasted tomato soup, 45 minutes, no cream needed",
            heart,
        ),
        saved_message(
            512,
            now - 6 * 86_400,
            "Long read: how Telegram syncs history across devices",
            heart,
        ),
    ];
    let json = format!(
        r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":4,"messages":[{}],"next_from_message_id":0}}"#,
        page.0,
        messages.join(",")
    );
    if let Some(owned) = copy_and_parse(&json, seq, sink) {
        session.apply(owned);
    }
}

/// The forum from the topics demo with the topic editor's icon choices
/// injected as the `getForumTopicDefaultIcons` answer.
fn open_editor(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    dyn_sink: &Arc<dyn DiagnosticSink>,
    seq: &AtomicU64,
) {
    super::groups_forum::seed_forum_chat_16(session, sink, seq);
    session.open_chat(ChatId(FORUM));
    let extra = session.request(RequestPurpose::GetForumTopicDefaultIcons, None);
    let stickers: Vec<String> = ["\u{1f4a1}", "\u{1f4f0}", "\u{1f3a8}", "\u{1f3b5}", "\u{1f4da}", "\u{2b50}", "\u{1f4ac}", "\u{1f3ae}", "\u{1f355}", "\u{1f680}", "\u{1f4f7}", "\u{1f4bc}"]
        .iter()
        .enumerate()
        .map(|(ix, emoji)| {
            let id = 5000 + ix as i64;
            format!(
                r#"{{"@type":"sticker","id":"{id}","set_id":"1","width":100,"height":100,"emoji":"{emoji}","format":{{"@type":"stickerFormatWebp"}},"full_type":{{"@type":"stickerFullTypeCustomEmoji","custom_emoji_id":"{id}","needs_repainting":false}},"thumbnail":null,"sticker":null}}"#
            )
        })
        .collect();
    let json = format!(
        r#"{{"@type":"stickers","@extra":"{}","stickers":[{}]}}"#,
        extra.0,
        stickers.join(",")
    );
    if let Some(owned) = copy_and_parse(&json, seq, dyn_sink) {
        session.apply(owned);
    }
}
