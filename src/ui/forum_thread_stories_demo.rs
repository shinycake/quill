//! `ready-forum-thread-stories` screenshot demo: the topic and thread info
//! cards, story statistics with public shares, and public story search.
//! Everything is injected through the normal reducer; no live Telegram.
//! `QUILL_DEMO_FTS_VIEW=topic|thread|stats|search` (default `topic`).

use super::demo::{demo_file_json, demo_thumb_png_path};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::{RequestPurpose, Session, StorySearchQuery};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn apply(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64, json: &str) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

fn user(id: i64, first: &str, last: &str) -> String {
    format!(
        r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"{last}","usernames":null,"phone_number":"","status":null,"profile_photo":null,"is_contact":false,"is_mutual_contact":false,"is_close_friend":false,"is_verified":false,"is_premium":false,"is_support":false,"restriction_reason":"","is_scam":false,"is_fake":false,"is_bot":false,"type":{{"@type":"userTypeRegular"}}}}}}"#
    )
}

pub(super) fn apply_ready_forum_thread_stories(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
    view: &str,
) {
    match view {
        "thread" => super::threads_demo::apply_ready_threads(session, sink, seq, "thread"),
        "stats" => apply_stats(session, sink, seq),
        "search" => apply_search(session, sink, seq),
        _ => apply_topic(session, sink, seq),
    }
}

fn apply_topic(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    apply(session, sink, seq, &user(6, "Dana", "Levi"));
    super::groups_forum::seed_forum_chat_16(session, sink, seq);
    session.open_chat(ChatId(16));
    session.select_topic(ChatId(16), 2);
    let extra = session.request_for_topic(RequestPurpose::GetTopicHistory, Some(ChatId(16)), 2);
    let json = format!(
        r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":1,"next_from_message_id":0,"messages":[{{"id":201,"chat_id":16,"is_outgoing":false,"topic_id":{{"@type":"messageTopicForum","forum_topic_id":2}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"v2.1 is rolling out this week.","entities":[]}}}}}}]}}"#,
        extra.0,
    );
    apply(session, sink, seq, &json);
}

fn photo_story(id: i32, chat_id: i64, date: i64, file: &str, extra: &str, caption: &str) -> String {
    format!(
        r#"{{"@type":"story","id":{id},"poster_chat_id":{chat_id},"date":{date},"content":{{"@type":"storyContentPhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"y","photo":{file},"width":960,"height":1280,"progressive_sizes":[]}}]}}}},{extra}"caption":{{"@type":"formattedText","text":{},"entities":[]}}}}"#,
        serde_json::to_string(caption).unwrap_or_default()
    )
}

fn graph(values: &[i64]) -> String {
    let xs: Vec<String> = (1..=values.len())
        .map(|i| (i * 86_400_000).to_string())
        .collect();
    let ys: Vec<String> = values.iter().map(i64::to_string).collect();
    let data = format!(
        r#"{{"columns":[["x",{}],["y0",{}]],"types":{{"x":"x","y0":"line"}}}}"#,
        xs.join(","),
        ys.join(",")
    );
    format!(
        r#"{{"@type":"statisticalGraphData","json_data":{},"zoom_token":""}}"#,
        serde_json::to_string(&data).unwrap_or_default()
    )
}

fn apply_stats(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    super::story_composer::apply_ready_story_post(session, sink, seq);
    let file = demo_file_json(91, &demo_thumb_png_path(), true);
    // The own story, now with statistics enabled.
    let story = photo_story(
        5,
        11,
        1_700_000_000,
        &file,
        r#""interaction_info":{"@type":"storyInteractionInfo","view_count":1280,"forward_count":9,"reaction_count":64,"recent_viewer_user_ids":[]},"can_be_deleted":true,"can_be_replied":true,"can_get_interactions":true,"can_get_statistics":true,"#,
        "Sunrise over the harbour.",
    );
    apply(session, sink, seq, &story);
    session.begin_story_insights(11, 5);
    let stats = session.request_for_story(RequestPurpose::GetStoryStatistics, ChatId(11), 5);
    let json = format!(
        r#"{{"@type":"storyStatistics","@extra":"{}","story_interaction_graph":{},"story_reaction_graph":{}}}"#,
        stats.0,
        graph(&[3, 9, 22, 41, 64, 58, 77, 90]),
        graph(&[1, 2, 6, 9, 14, 12, 18, 21]),
    );
    apply(session, sink, seq, &json);
    let forwards = session.request_for_story(RequestPurpose::GetStoryPublicForwards, ChatId(11), 5);
    let json = format!(
        r#"{{"@type":"publicForwards","@extra":"{}","total_count":3,"next_offset":"","forwards":[{{"@type":"publicForwardMessage","message":{{"id":300,"chat_id":13,"date":{}}}}},{{"@type":"publicForwardStory","story":{{"id":2,"poster_chat_id":12,"date":{}}}}},{{"@type":"publicForwardMessage","message":{{"id":55,"chat_id":16,"date":{}}}}}]}}"#,
        forwards.0,
        super::format_helpers::now_unix_secs() - 3_600,
        super::format_helpers::now_unix_secs() - 86_400,
        super::format_helpers::now_unix_secs() - 3 * 86_400,
    );
    apply(session, sink, seq, &json);
}

fn apply_search(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    super::story_viewer::apply_ready_stories(session, sink, seq);
    session.open_search();
    let search_gen = session.search.begin_query("#sunset");
    let chats = session.request_search(RequestPurpose::SearchChats, search_gen);
    let messages = session.request_search(RequestPurpose::SearchMessages, search_gen);
    apply(
        session,
        sink,
        seq,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":0,"chat_ids":[]}}"#,
            chats.0
        ),
    );
    apply(
        session,
        sink,
        seq,
        &format!(
            r#"{{"@type":"foundMessages","@extra":"{}","total_count":0,"next_offset":"","messages":[]}}"#,
            messages.0
        ),
    );
    session.begin_story_search(StorySearchQuery::Tag("#sunset".into()));
    let extra = session.request(RequestPurpose::SearchPublicStories, None);
    let file = demo_file_json(91, &demo_thumb_png_path(), true);
    let now = super::format_helpers::now_unix_secs();
    let stories = [
        photo_story(7, 11, now - 1_800, &file, "", "Golden hour."),
        photo_story(8, 12, now - 7_200, &file, "", "#sunset from the roof."),
        photo_story(9, 13, now - 86_400, &file, "", "Another #sunset."),
    ]
    .join(",");
    let json = format!(
        r#"{{"@type":"foundStories","@extra":"{}","total_count":3,"next_offset":"","stories":[{stories}]}}"#,
        extra.0
    );
    apply(session, sink, seq, &json);
}
