//! Connect-driver tests: subsection tabs (bots with topics).
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::ChatId;
use crate::platform::MemorySecretStore;
use crate::settings::load_media_prefs;
use crate::subsection_tabs::SubsectionTabsMode;
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn bot_user_json(id: i64, has_topics: bool, allows: bool) -> String {
    format!(
        r#"{{"@type":"updateUser","user":{{"id":{id},"first_name":"Bot","type":{{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":{has_topics},"allows_users_to_create_topics":{allows},"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}}}}"#
    )
}

fn bot_chat_json(id: i64) -> String {
    format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"Bot","type":{{"@type":"chatTypePrivate","user_id":{id}}},"unread_count":0}}}}"#
    )
}

fn sent(recorder: &RecordingSender, ty: &str) -> Vec<Value> {
    recorder
        .snapshot()
        .into_iter()
        .map(|j| serde_json::from_str::<Value>(&j).unwrap())
        .filter(|v| v["@type"] == ty)
        .collect()
}

/// A bot chat with `has_topics` loads its topics as soon as it is known
/// (for the chat-row topic line) and again is deduped on open; topic
/// selection is allowed and pages the topic's history.
#[test]
fn bot_with_topics_fetches_topics_and_selects_topic() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    };
    // A plain bot: no topic fetch.
    ingest(&mut driver, &bot_user_json(51, false, false));
    ingest(&mut driver, &bot_chat_json(51));
    assert!(sent(&recorder, "getForumTopics").is_empty());
    assert!(!driver.session.chat_has_topics(ChatId(51)));

    ingest(&mut driver, &bot_user_json(41, true, true));
    ingest(&mut driver, &bot_chat_json(41));
    let fetches = sent(&recorder, "getForumTopics");
    assert_eq!(fetches.len(), 1, "one getForumTopics for the bot chat");
    assert_eq!(fetches[0]["chat_id"], 41);
    assert!(driver.session.chat_has_topics(ChatId(41)));
    // Opening the chat while the fetch is in flight doesn't double it.
    driver.select_chat(ChatId(41)).unwrap();
    assert_eq!(sent(&recorder, "getForumTopics").len(), 1);

    let extra = fetches[0]["@extra"].as_str().unwrap().to_string();
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"forumTopics","@extra":"{extra}","total_count":1,"topics":[{{"info":{{"@type":"forumTopicInfo","chat_id":41,"forum_topic_id":7,"name":"Trips","icon":{{"@type":"forumTopicIcon","color":7322096,"custom_emoji_id":"0"}},"is_general":false,"is_closed":false,"is_hidden":false}},"last_message":{{"id":90,"chat_id":41,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}},"order":"5","is_pinned":false,"unread_count":2,"last_read_inbox_message_id":80}}],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}}"#
        ),
    );
    let topics = driver.session.ordered_forum_topics(ChatId(41));
    assert_eq!(topics[0].icon_color, 7322096);
    assert_eq!(topics[0].last_message_id, 90);
    assert!(driver.session.subsection_tabs_used_for(ChatId(41)));

    driver.select_topic(7).unwrap();
    let search = sent(&recorder, "searchChatMessages");
    assert_eq!(search.last().unwrap()["topic_id"]["forum_topic_id"], 7);

    // Mark as read: `viewMessages` on the topic's last message.
    driver.mark_forum_topic_read(ChatId(41), 7).unwrap();
    let view = sent(&recorder, "viewMessages");
    let view = view.last().unwrap();
    assert_eq!(view["message_ids"][0], 90);
    assert_eq!(view["source"]["@type"], "messageSourceForumTopicHistory");
    assert_eq!(
        driver.session.ordered_forum_topics(ChatId(41))[0].unread_count,
        0
    );

    // Mute: `setForumTopicNotificationSettings` with only `mute_for` set.
    driver.set_forum_topic_muted(ChatId(41), 7, true).unwrap();
    let mute = sent(&recorder, "setForumTopicNotificationSettings");
    assert_eq!(mute[0]["forum_topic_id"], 7);
    assert_eq!(
        mute[0]["notification_settings"]["use_default_mute_for"],
        false
    );
    assert!(
        driver.session.ordered_forum_topics(ChatId(41))[0]
            .notification_settings
            .is_muted()
    );

    // Pin works in a bot chat (no supergroup admin rights involved).
    assert!(
        driver
            .toggle_forum_topic_pinned(ChatId(41), 7, true)
            .unwrap()
            .is_some()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The layout toggle cycles Top → Bottom → Left → Top and persists per
/// chat in `media_prefs.json`.
#[test]
fn tabs_mode_cycles_and_persists() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let paths = prepared.paths.clone();
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    let chat = ChatId(41);
    assert_eq!(
        driver.session.subsection_tabs_mode(chat),
        SubsectionTabsMode::Top
    );
    assert_eq!(
        driver.cycle_subsection_tabs_mode(chat),
        SubsectionTabsMode::Bottom
    );
    assert_eq!(
        load_media_prefs(&paths).subsection_tabs_modes.get(&41),
        Some(&SubsectionTabsMode::Bottom)
    );
    assert_eq!(
        driver.cycle_subsection_tabs_mode(chat),
        SubsectionTabsMode::Left
    );
    assert_eq!(
        load_media_prefs(&paths).subsection_tabs_modes.get(&41),
        Some(&SubsectionTabsMode::Left)
    );
    // Other chats keep the default.
    assert_eq!(
        driver.session.subsection_tabs_mode(ChatId(42)),
        SubsectionTabsMode::Top
    );
    assert_eq!(
        driver.cycle_subsection_tabs_mode(chat),
        SubsectionTabsMode::Top
    );
    assert!(load_media_prefs(&paths).subsection_tabs_modes.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
