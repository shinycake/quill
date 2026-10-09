//! Connect-driver tests: forum view mode / topic extras, Saved Messages
//! sublists and tags (batch B16). All traffic is recorded JSON; no account.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::ChatId;
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::ReactionType;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const ME: i64 = 13;

struct Fixture {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    dyn_sink: Arc<dyn DiagnosticSink>,
    seq: AtomicU64,
    dir: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
        Self {
            driver,
            recorder,
            dyn_sink,
            seq,
            dir,
        }
    }

    fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.dyn_sink).unwrap())
            .unwrap();
    }

    fn sent(&self, ty: &str) -> Vec<Value> {
        self.recorder
            .snapshot()
            .into_iter()
            .map(|j| serde_json::from_str::<Value>(&j).unwrap())
            .filter(|v| v["@type"] == ty)
            .collect()
    }

    /// Answer the latest request of type `ty` with `body` (an object
    /// fragment without the opening brace's `@extra`).
    fn answer(&mut self, ty: &str, body: &str) {
        let extra = self
            .sent(ty)
            .last()
            .unwrap_or_else(|| panic!("{ty} was not sent"))["@extra"]
            .as_str()
            .unwrap()
            .to_owned();
        self.ingest(&body.replace("EXTRA", &extra));
    }

    fn saved_chat(&mut self) {
        self.ingest(&format!(
            r#"{{"@type":"updateOption","name":"my_id","value":{{"@type":"optionValueInteger","value":"{ME}"}}}}"#
        ));
        self.ingest(&format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{ME},"title":"Saved","type":{{"@type":"chatTypePrivate","user_id":{ME}}},"unread_count":0}}}}"#
        ));
    }

    fn forum(&mut self, manage: bool) {
        self.ingest(
            r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Forum","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"unread_count":0}}"#,
        );
        self.ingest(
            r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":16,"is_forum":true}}"#,
        );
        if manage {
            self.driver
                .session
                .supergroup_manage_topics_right
                .insert(16, true);
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn view_as_topics_toggle_sends_and_applies_the_update() {
    let mut f = Fixture::new();
    f.forum(false);
    assert!(f.driver.session.chat_views_as_topics(ChatId(16)));
    f.driver.toggle_view_as_topics(ChatId(16), false).unwrap();
    let sent = f.sent("toggleChatViewAsTopics");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["chat_id"], 16);
    assert_eq!(sent[0]["view_as_topics"], false);
    // A second tap while one is in flight is deduped.
    assert!(
        f.driver
            .toggle_view_as_topics(ChatId(16), false)
            .unwrap()
            .is_none()
    );
    // Nothing changes until TDLib confirms with the update.
    assert!(f.driver.session.chat_views_as_topics(ChatId(16)));
    f.ingest(r#"{"@type":"updateChatViewAsTopics","chat_id":16,"view_as_topics":false}"#);
    assert!(!f.driver.session.chat_views_as_topics(ChatId(16)));
}

#[test]
fn topic_link_answer_reaches_the_clipboard_slot() {
    let mut f = Fixture::new();
    f.forum(false);
    f.driver.copy_forum_topic_link(ChatId(16), 7).unwrap();
    let sent = f.sent("getForumTopicLink");
    assert_eq!(sent[0]["forum_topic_id"], 7);
    f.answer(
        "getForumTopicLink",
        r#"{"@type":"messageLink","@extra":"EXTRA","link":"https://t.me/forum/7","is_public":true}"#,
    );
    assert_eq!(
        f.driver.session.message_link_result.as_deref(),
        Some("https://t.me/forum/7")
    );
    assert!(f.driver.session.message_link_public);
}

#[test]
fn moving_a_pinned_topic_sends_the_new_order_and_reorders_locally() {
    let mut f = Fixture::new();
    f.forum(true);
    let extra = f.driver.session.request(
        crate::state::RequestPurpose::GetForumTopics,
        Some(ChatId(16)),
    );
    let topic = |id: i32, order: i64| {
        format!(
            r#"{{"info":{{"@type":"forumTopicInfo","chat_id":16,"forum_topic_id":{id},"name":"T{id}","icon":{{"@type":"forumTopicIcon","color":0,"custom_emoji_id":"0"}},"is_general":false,"is_closed":false,"is_hidden":false}},"last_message":null,"order":"{order}","is_pinned":true,"unread_count":0,"last_read_inbox_message_id":0,"notification_settings":{{"@type":"chatNotificationSettings"}}}}"#
        )
    };
    f.ingest(&format!(
        r#"{{"@type":"forumTopics","@extra":"{}","total_count":2,"topics":[{},{}],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}}"#,
        extra.0,
        topic(1, 900),
        topic(2, 800)
    ));
    f.driver
        .move_pinned_forum_topic(ChatId(16), 2, true)
        .unwrap();
    let sent = f.sent("setPinnedForumTopics");
    assert_eq!(sent[0]["forum_topic_ids"], serde_json::json!([2, 1]));
    assert_eq!(
        f.driver.session.pinned_forum_topic_ids(ChatId(16)),
        vec![2, 1]
    );
    // The top topic cannot move up: nothing is sent.
    assert!(
        f.driver
            .move_pinned_forum_topic(ChatId(16), 2, true)
            .unwrap()
            .is_none()
    );
    assert_eq!(f.sent("setPinnedForumTopics").len(), 1);
}

#[test]
fn reading_marks_and_unpinning_in_a_topic_send_their_requests() {
    let mut f = Fixture::new();
    f.forum(false);
    f.ingest(
        r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Forum","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"unread_count":0}}"#,
    );
    f.driver
        .read_all_forum_topic_mentions(ChatId(16), 3)
        .unwrap();
    f.driver
        .read_all_forum_topic_reactions(ChatId(16), 3)
        .unwrap();
    f.driver
        .unpin_all_forum_topic_messages(ChatId(16), 3)
        .unwrap();
    for ty in [
        "readAllForumTopicMentions",
        "readAllForumTopicReactions",
        "unpinAllForumTopicMessages",
    ] {
        let sent = f.sent(ty);
        assert_eq!(sent.len(), 1, "{ty}");
        assert_eq!(sent[0]["chat_id"], 16);
        assert_eq!(sent[0]["forum_topic_id"], 3);
    }
}

#[test]
fn topic_icons_load_once_and_create_edit_carry_the_icon() {
    let mut f = Fixture::new();
    f.forum(true);
    f.driver.load_forum_topic_icons().unwrap();
    assert_eq!(f.sent("getForumTopicDefaultIcons").len(), 1);
    // Deduped while in flight.
    assert!(f.driver.load_forum_topic_icons().unwrap().is_none());

    f.driver
        .create_forum_topic_with_icon(ChatId(16), "  Ideas ", 0xFFD67E, 5005)
        .unwrap();
    let created = f.sent("createForumTopic");
    assert_eq!(created[0]["name"], "Ideas");
    assert_eq!(created[0]["icon"]["color"], 0xFFD67E);
    assert_eq!(created[0]["icon"]["custom_emoji_id"], 5005);
    // A color outside the six tdesktop colors is refused up front.
    assert!(
        f.driver
            .create_forum_topic_with_icon(ChatId(16), "x", 0x123456, 0)
            .is_err()
    );
    assert!(
        f.driver
            .create_forum_topic_with_icon(ChatId(16), "  ", 0xFFD67E, 0)
            .is_err()
    );
    f.driver
        .edit_forum_topic_with_icon(ChatId(16), 3, "Plans", 0)
        .unwrap();
    let edited = f.sent("editForumTopic");
    assert_eq!(edited[0]["edit_icon_custom_emoji"], true);
    assert_eq!(edited[0]["icon_custom_emoji_id"], 0);
    assert_eq!(edited[0]["name"], "Plans");
}

#[test]
fn saved_sublists_load_open_page_pin_and_delete() {
    let mut f = Fixture::new();
    f.saved_chat();
    f.driver.load_saved_topics().unwrap();
    assert_eq!(f.sent("loadSavedMessagesTopics").len(), 1);
    assert!(f.driver.load_saved_topics().unwrap().is_none(), "deduped");
    f.answer(
        "loadSavedMessagesTopics",
        r#"{"@type":"ok","@extra":"EXTRA"}"#,
    );
    f.ingest(
        r#"{"@type":"updateSavedMessagesTopic","topic":{"@type":"savedMessagesTopic","id":"77","type":{"@type":"savedMessagesTopicTypeSavedFromChat","chat_id":77},"is_pinned":false,"order":"5"}}"#,
    );
    // The next ask after a 404 is silent.
    f.driver.load_saved_topics().unwrap();
    f.answer(
        "loadSavedMessagesTopics",
        r#"{"@type":"error","@extra":"EXTRA","code":404,"message":"Not Found"}"#,
    );
    assert!(f.driver.session.saved.topics_exhausted);
    assert!(f.driver.load_saved_topics().unwrap().is_none());

    f.driver.open_saved_sublist(77).unwrap();
    let history = f.sent("getSavedMessagesTopicHistory");
    assert_eq!(history[0]["saved_messages_topic_id"], 77);
    assert_eq!(history[0]["from_message_id"], 0);
    assert_eq!(f.sent("getSavedMessagesTags").len(), 1);
    f.answer(
        "getSavedMessagesTopicHistory",
        &format!(
            r#"{{"@type":"messages","@extra":"EXTRA","total_count":1,"messages":[{{"id":50,"chat_id":{ME},"date":1,"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}}]}}"#
        ),
    );
    // The next page continues from the oldest id.
    f.driver.fetch_saved_sublist_history().unwrap();
    assert_eq!(
        f.sent("getSavedMessagesTopicHistory")[1]["from_message_id"],
        50
    );

    f.driver.toggle_saved_topic_pinned(77, true).unwrap();
    assert_eq!(
        f.sent("toggleSavedMessagesTopicIsPinned")[0]["is_pinned"],
        true
    );
    f.driver.delete_saved_topic_history(77).unwrap();
    f.answer(
        "deleteSavedMessagesTopicHistory",
        r#"{"@type":"ok","@extra":"EXTRA"}"#,
    );
    assert!(f.driver.session.saved.topics.is_empty());
    assert!(f.driver.session.saved.sublist.is_none());
}

#[test]
fn tag_label_needs_premium_and_filter_searches_saved_messages() {
    let mut f = Fixture::new();
    f.saved_chat();
    let tag = ReactionType::emoji("\u{2764}");
    // Not Premium: the rename is refused before anything is sent.
    f.ingest(&format!(
        r#"{{"@type":"updateUser","user":{{"id":{ME},"first_name":"Me","is_premium":false,"type":{{"@type":"userTypeRegular"}}}}}}"#
    ));
    assert!(f.driver.set_saved_tag_label(&tag, "Love").is_err());
    assert!(f.sent("setSavedMessagesTagLabel").is_empty());

    f.ingest(&format!(
        r#"{{"@type":"updateUser","user":{{"id":{ME},"first_name":"Me","is_premium":true,"type":{{"@type":"userTypeRegular"}}}}}}"#
    ));
    f.ingest(
        r#"{"@type":"updateSavedMessagesTags","saved_messages_topic_id":"0","tags":{"@type":"savedMessagesTags","tags":[{"tag":{"@type":"reactionTypeEmoji","emoji":"❤"},"label":"","count":2}]}}"#,
    );
    f.driver
        .set_saved_tag_label(&tag, "  A very long tag label  ")
        .unwrap();
    let sent = f.sent("setSavedMessagesTagLabel");
    assert_eq!(sent[0]["label"], "A very long");
    assert_eq!(sent[0]["tag"]["@type"], "reactionTypeEmoji");
    // 12 characters at most, and the bar shows the name at once.
    assert!(sent[0]["label"].as_str().unwrap().chars().count() <= 12);
    assert_eq!(f.driver.session.saved.tags[0].label, "A very long");

    // Filter by tag: searchSavedMessages over all of Saved Messages.
    f.driver.filter_saved_by_tag(tag.clone()).unwrap();
    let search = f.sent("searchSavedMessages");
    assert_eq!(search[0]["saved_messages_topic_id"], 0);
    assert_eq!(search[0]["tag"]["emoji"], "\u{2764}");
    f.answer(
        "searchSavedMessages",
        &format!(
            r#"{{"@type":"foundChatMessages","@extra":"EXTRA","total_count":1,"messages":[{{"id":9,"chat_id":{ME},"date":1,"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"tagged","entities":[]}}}}}}],"next_from_message_id":0}}"#
        ),
    );
    let found = f.driver.session.saved.tag_search.as_ref().unwrap();
    assert_eq!(found.history.messages.len(), 1);
    assert!(found.history.loaded_complete);
    // Inside a sublist the same filter is scoped to that sublist.
    f.driver.clear_saved_tag_filter();
    f.driver.open_saved_sublist(77).unwrap();
    f.driver.filter_saved_by_tag(tag).unwrap();
    assert_eq!(
        f.sent("searchSavedMessages")[1]["saved_messages_topic_id"],
        77
    );
}
