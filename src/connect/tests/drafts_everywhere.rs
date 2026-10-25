//! Connect-driver tests: drafts in groups, bots, channels, forum topics and
//! reply threads — request shape per slot, keyed save / restore across
//! topic and thread switches (recorded TDLib JSON in, requests out).
use super::super::*;
use super::*;
use crate::composer::DraftTopic;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{ChatId, MessageId};
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::ChatDraft;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

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

    fn drafts_sent(&self) -> Vec<Value> {
        self.sent("setChatDraftMessage")
    }

    fn last_draft_sent(&self) -> Value {
        self.drafts_sent()
            .last()
            .cloned()
            .expect("a draft was sent")
    }

    /// Answer the latest request of type `ty` with `body` (`EXTRA` is
    /// replaced by that request's `@extra`).
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

    fn chat(&mut self, id: i64, kind: &str) {
        self.ingest(&format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"C{id}","type":{kind},"unread_count":0}}}}"#
        ));
    }

    /// Forum 16 (topics 3 and 4, topic 3 carrying a draft), opened.
    fn open_forum(&mut self) {
        self.chat(
            16,
            r#"{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false}"#,
        );
        self.ingest(
            r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":16,"is_forum":true,"status":{"@type":"chatMemberStatusMember"}}}"#,
        );
        self.driver.select_chat(ChatId(16)).unwrap();
        let topic = |id: i32, draft: &str| {
            format!(
                r#"{{"info":{{"@type":"forumTopicInfo","chat_id":16,"forum_topic_id":{id},"name":"T{id}","icon":{{"@type":"forumTopicIcon","color":0,"custom_emoji_id":"0"}},"is_general":false,"is_closed":false,"is_hidden":false}},"last_message":null,"order":"{order}","is_pinned":false,"unread_count":0,"last_read_inbox_message_id":0,"notification_settings":{{"@type":"chatNotificationSettings"}},"draft_message":{draft}}}"#,
                order = 1000 - i64::from(id)
            )
        };
        let draft = r#"{"@type":"draftMessage","reply_to":null,"date":5,"content":{"@type":"draftMessageContentText","text":{"@type":"formattedText","text":"topic three","entities":[]},"link_preview_options":null},"effect_id":"0","suggested_post_info":null}"#;
        self.answer(
            "getForumTopics",
            &format!(
                r#"{{"@type":"forumTopics","@extra":"EXTRA","total_count":2,"topics":[{},{}],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}}"#,
                topic(3, draft),
                topic(4, "null"),
            ),
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn text_draft(text: &str) -> ChatDraft {
    ChatDraft {
        text: text.into(),
        rich: None,
        item: Default::default(),
        reply_to_message_id: None,
        quote: None,
        suggested_post: None,
    }
}

#[test]
fn groups_and_bots_save_chat_drafts_and_a_read_only_channel_does_not() {
    let mut f = Fixture::new();
    f.chat(8, r#"{"@type":"chatTypeBasicGroup","basic_group_id":8}"#);
    f.chat(12, r#"{"@type":"chatTypePrivate","user_id":12}"#);
    f.ingest(
        r#"{"@type":"updateUser","user":{"id":12,"first_name":"Bot","type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":false,"allows_users_to_create_topics":false,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#,
    );
    f.chat(
        10,
        r#"{"@type":"chatTypeSupergroup","supergroup_id":10,"is_channel":true}"#,
    );
    f.ingest(
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":10,"is_channel":true,"status":{"@type":"chatMemberStatusMember"}}}"#,
    );

    f.driver.select_chat(ChatId(8)).unwrap();
    assert_eq!(
        f.driver
            .note_composer_draft(ChatId(8), "to the group", None, 1_000, false)
            .unwrap(),
        DraftSaveOutcome::Sent
    );
    let sent = f.last_draft_sent();
    assert_eq!(sent["chat_id"], 8);
    assert_eq!(sent["topic_id"], Value::Null);
    assert_eq!(
        sent["draft_message"]["content"]["text"]["text"],
        "to the group"
    );
    assert_eq!(
        f.driver.session.draft_for(ChatId(8), DraftTopic::Chat),
        Some(&text_draft("to the group"))
    );
    assert_eq!(
        f.driver.session.chats.get(&8).unwrap().sidebar_preview(),
        "Draft: to the group"
    );

    f.driver.select_chat(ChatId(12)).unwrap();
    assert_eq!(
        f.driver
            .note_composer_draft(ChatId(12), "/start soon", None, 2_000, false)
            .unwrap(),
        DraftSaveOutcome::Sent
    );
    assert_eq!(f.last_draft_sent()["chat_id"], 12);

    f.driver.select_chat(ChatId(10)).unwrap();
    assert_eq!(
        f.driver
            .note_composer_draft(ChatId(10), "cannot post", None, 3_000, false)
            .unwrap(),
        DraftSaveOutcome::Skipped
    );
    assert_eq!(f.drafts_sent().len(), 2);
    assert!(f.driver.session.chats.get(&10).unwrap().draft.is_none());
}

#[test]
fn forum_topics_draft_per_topic_with_message_topic_forum() {
    let mut f = Fixture::new();
    f.open_forum();
    // The forum's own slot is off: a flush from the "All" view sends nothing.
    assert_eq!(
        f.driver
            .note_composer_draft(ChatId(16), "all view", None, 1_000, false)
            .unwrap(),
        DraftSaveOutcome::Skipped
    );
    assert!(f.drafts_sent().is_empty());

    f.driver.select_topic(3).unwrap();
    assert_eq!(
        f.driver.session.open_draft_topic(ChatId(16)),
        DraftTopic::Forum(3)
    );
    // Topic 3 came with a draft from `forumTopic.draft_message`.
    assert_eq!(
        f.driver.session.draft_for(ChatId(16), DraftTopic::Forum(3)),
        Some(&text_draft("topic three"))
    );
    // Typing the same text back is not a save.
    assert_eq!(
        f.driver
            .note_composer_draft(ChatId(16), "topic three", None, 1_100, true)
            .unwrap(),
        DraftSaveOutcome::Skipped
    );
    assert_eq!(
        f.driver
            .note_composer_draft(ChatId(16), "topic three, edited", None, 1_200, false)
            .unwrap(),
        DraftSaveOutcome::Sent
    );
    let sent = f.last_draft_sent();
    assert_eq!(sent["chat_id"], 16);
    assert_eq!(sent["topic_id"]["@type"], "messageTopicForum");
    assert_eq!(sent["topic_id"]["forum_topic_id"], 3);
    assert_eq!(
        sent["draft_message"]["content"]["text"]["text"],
        "topic three, edited"
    );
    assert_eq!(
        f.driver.session.draft_for(ChatId(16), DraftTopic::Forum(3)),
        Some(&text_draft("topic three, edited"))
    );
    assert_eq!(
        f.driver.session.draft_for(ChatId(16), DraftTopic::Forum(4)),
        None
    );
    assert!(f.driver.session.chats.get(&16).unwrap().draft.is_none());

    // Switch to topic 4: the UI flushes with the slot it read before the
    // switch; the explicit form lands in topic 3 even after the switch.
    f.driver.select_topic(4).unwrap();
    assert_eq!(
        f.driver.session.open_draft_topic(ChatId(16)),
        DraftTopic::Forum(4)
    );
    assert_eq!(
        f.driver
            .note_composer_draft_in(
                ChatId(16),
                DraftTopic::Forum(3),
                "late flush for three",
                None,
                2_000,
                false,
            )
            .unwrap(),
        DraftSaveOutcome::Sent
    );
    assert_eq!(f.last_draft_sent()["topic_id"]["forum_topic_id"], 3);
    assert_eq!(
        f.driver
            .note_composer_draft(ChatId(16), "four", None, 2_100, false)
            .unwrap(),
        DraftSaveOutcome::Sent
    );
    assert_eq!(f.last_draft_sent()["topic_id"]["forum_topic_id"], 4);
    assert_eq!(
        f.driver.session.draft_for(ChatId(16), DraftTopic::Forum(4)),
        Some(&text_draft("four"))
    );
    // The topic row shows its draft.
    let topics = f.driver.session.ordered_forum_topics(ChatId(16));
    let four = topics.iter().find(|t| t.forum_topic_id == 4).unwrap();
    assert_eq!(four.draft_preview().unwrap().text, "four");

    // A send into topic 4 clears that slot only.
    f.ingest(
        r#"{"@type":"updateMessageSendSucceeded","message":{"id":9,"chat_id":16,"is_outgoing":true,"topic_id":{"@type":"messageTopicForum","forum_topic_id":4},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"four","entities":[]}}},"old_message_id":-1}"#,
    );
    assert_eq!(
        f.driver.session.messages.draft_clears,
        vec![(ChatId(16), DraftTopic::Forum(4))]
    );
    f.driver
        .clear_draft_after_send(ChatId(16), DraftTopic::Forum(4), true)
        .unwrap()
        .expect("clear sent");
    let sent = f.last_draft_sent();
    assert_eq!(sent["topic_id"]["forum_topic_id"], 4);
    assert_eq!(sent["draft_message"], Value::Null);
    assert_eq!(
        f.driver.session.draft_for(ChatId(16), DraftTopic::Forum(4)),
        None
    );
    assert_eq!(
        f.driver.session.draft_for(ChatId(16), DraftTopic::Forum(3)),
        Some(&text_draft("late flush for three"))
    );
    // Nothing to clear: no request.
    assert!(
        f.driver
            .clear_draft_after_send(ChatId(16), DraftTopic::Forum(4), true)
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_debounced_topic_draft_keeps_its_slot_across_a_topic_switch() {
    let mut f = Fixture::new();
    f.open_forum();
    f.driver.select_topic(4).unwrap();
    let DraftSaveOutcome::Debounced { token, .. } = f
        .driver
        .note_composer_draft(ChatId(16), "typing in four", None, 1_000, true)
        .unwrap()
    else {
        panic!("expected a debounce");
    };
    assert!(
        f.driver
            .session
            .draft_is_dirty(ChatId(16), DraftTopic::Forum(4))
    );
    // A remote draft for the dirty topic is ignored meanwhile.
    f.ingest(
        r#"{"@type":"updateForumTopic","chat_id":16,"forum_topic_id":4,"is_pinned":false,"last_read_inbox_message_id":0,"last_read_outbox_message_id":0,"unread_mention_count":0,"unread_reaction_count":0,"notification_settings":{"@type":"chatNotificationSettings"},"draft_message":{"@type":"draftMessage","reply_to":null,"date":9,"content":{"@type":"draftMessageContentText","text":{"@type":"formattedText","text":"stale","entities":[]},"link_preview_options":null},"effect_id":"0","suggested_post_info":null}}"#,
    );
    assert_eq!(
        f.driver.session.draft_for(ChatId(16), DraftTopic::Forum(4)),
        None
    );
    f.driver.select_topic(3).unwrap();
    f.driver
        .commit_debounced_draft(token)
        .unwrap()
        .expect("sent");
    let sent = f.last_draft_sent();
    assert_eq!(sent["topic_id"]["forum_topic_id"], 4);
    assert_eq!(
        sent["draft_message"]["content"]["text"]["text"],
        "typing in four"
    );
    assert_eq!(
        f.driver.session.draft_for(ChatId(16), DraftTopic::Forum(4)),
        Some(&text_draft("typing in four"))
    );
    assert!(
        !f.driver
            .session
            .draft_is_dirty(ChatId(16), DraftTopic::Forum(4))
    );
}

fn thread_message(id: i64, chat: i64, thread: i64, text: &str) -> String {
    format!(
        r#"{{"@type":"message","id":{id},"chat_id":{chat},"is_outgoing":false,"date":1700000000,"topic_id":{{"@type":"messageTopicThread","message_thread_id":{thread}}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}}"#
    )
}

#[test]
fn a_comments_thread_drafts_with_message_topic_thread() {
    let mut f = Fixture::new();
    f.chat(
        13,
        r#"{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true}"#,
    );
    f.chat(
        14,
        r#"{"@type":"chatTypeSupergroup","supergroup_id":14,"is_channel":false}"#,
    );
    f.driver.select_chat(ChatId(13)).unwrap();
    f.driver.open_thread(ChatId(13), MessageId(101)).unwrap();
    let draft = r#"{"@type":"draftMessage","reply_to":null,"date":5,"content":{"@type":"draftMessageContentText","text":{"@type":"formattedText","text":"my comment","entities":[]},"link_preview_options":null},"effect_id":"0","suggested_post_info":null}"#;
    f.answer(
        "getMessageThread",
        &format!(
            r#"{{"@type":"messageThreadInfo","@extra":"EXTRA","chat_id":14,"message_thread_id":501,"reply_info":{{"@type":"messageReplyInfo","reply_count":3,"recent_replier_ids":[],"last_read_inbox_message_id":0,"last_read_outbox_message_id":0,"last_message_id":504}},"unread_message_count":0,"messages":[{}],"draft_message":{draft}}}"#,
            thread_message(501, 14, 501, "the post")
        ),
    );
    f.driver.switch_to_thread_chat().unwrap();
    assert_eq!(f.driver.session.open_chat, Some(ChatId(14)));
    assert_eq!(
        f.driver.session.open_draft_topic(ChatId(14)),
        DraftTopic::Thread(501)
    );
    // The thread draft from `messageThreadInfo` is what the composer restores.
    assert_eq!(
        f.driver
            .session
            .draft_for(ChatId(14), DraftTopic::Thread(501)),
        Some(&text_draft("my comment"))
    );
    assert_eq!(
        f.driver.session.draft_for(ChatId(14), DraftTopic::Chat),
        None
    );

    assert_eq!(
        f.driver
            .note_composer_draft(ChatId(14), "my comment, longer", None, 1_000, false)
            .unwrap(),
        DraftSaveOutcome::Sent
    );
    let sent = f.last_draft_sent();
    assert_eq!(sent["chat_id"], 14);
    assert_eq!(sent["topic_id"]["@type"], "messageTopicThread");
    assert_eq!(sent["topic_id"]["message_thread_id"], 501);
    assert_eq!(
        f.driver
            .session
            .draft_for(ChatId(14), DraftTopic::Thread(501)),
        Some(&text_draft("my comment, longer"))
    );
    // The discussion group's own row never shows the thread draft.
    assert!(f.driver.session.chats.get(&14).unwrap().draft.is_none());

    // A reply sent into the thread clears the thread slot.
    f.ingest(&format!(
        r#"{{"@type":"updateMessageSendSucceeded","message":{},"old_message_id":-1}}"#,
        thread_message(505, 14, 501, "my comment, longer")
            .replace(r#""is_outgoing":false"#, r#""is_outgoing":true"#)
    ));
    assert_eq!(
        f.driver.session.messages.draft_clears,
        vec![(ChatId(14), DraftTopic::Thread(501))]
    );
    f.driver
        .clear_draft_after_send(ChatId(14), DraftTopic::Thread(501), true)
        .unwrap()
        .expect("clear sent");
    let sent = f.last_draft_sent();
    assert_eq!(sent["topic_id"]["message_thread_id"], 501);
    assert_eq!(sent["draft_message"], Value::Null);

    // Leaving the thread: back to the chat slot.
    assert_eq!(f.driver.close_thread(), Some(ChatId(13)));
    assert_eq!(
        f.driver.session.open_draft_topic(ChatId(14)),
        DraftTopic::Chat
    );
}
