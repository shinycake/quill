//! Connect-driver tests: a refused request ends its loading state and
//! rolls back what was shown before the answer (similar bots, topic
//! mute, poll votes), and a failed sticker set is not asked again on
//! every update.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{ChatId, MessageId, RequestId};
use crate::platform::MemorySecretStore;
use crate::state::{SimilarBotsFetch, StickerTab};
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::MessageContent;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

struct Rig {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    seq: AtomicU64,
    sink: Arc<dyn DiagnosticSink>,
    _dir: std::path::PathBuf,
}

impl Rig {
    fn new() -> Self {
        let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
        seed_ready_alice(&mut driver, &seq, &sink);
        Self {
            driver,
            recorder,
            seq,
            sink,
            _dir: dir,
        }
    }

    fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.sink).unwrap())
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

    fn refuse(&mut self, extra: RequestId, message: &str) {
        self.ingest(&format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"{message}"}}"#,
            extra.0
        ));
    }
}

#[test]
fn a_failed_sticker_set_is_not_asked_again_until_another_is_picked() {
    let mut rig = Rig::new();
    let panel = &mut rig.driver.session.stickers.stickers;
    panel.open = true;
    panel.tab = StickerTab::Installed;
    let asked = rig.driver.select_sticker_set(77).unwrap().unwrap();
    rig.refuse(asked, "STICKERSET_INVALID");
    // The error envelope and later updates do not send it again.
    rig.ingest(
        r#"{"@type":"updateUserStatus","user_id":7,"status":{"@type":"userStatusOnline","expires":1}}"#,
    );
    assert_eq!(rig.sent("getStickerSet").len(), 1);
    assert_eq!(rig.driver.session.stickers.stickers.failed_set_id, Some(77));
    // Another set, then back: asked again (the user chose it).
    let other = rig.driver.select_sticker_set(88).unwrap().unwrap();
    rig.refuse(other, "STICKERSET_INVALID");
    rig.driver.select_sticker_set(77).unwrap();
    assert_eq!(rig.sent("getStickerSet").len(), 3);
}

#[test]
fn a_failed_similar_bots_load_can_be_asked_again() {
    let mut rig = Rig::new();
    let asked = rig.driver.fetch_similar_bots(42).unwrap().unwrap();
    // The UI marks the section loading once the request is out.
    rig.driver
        .session
        .bots
        .similar_bots
        .insert(42, SimilarBotsFetch::Loading);
    rig.refuse(asked, "BOT_INVALID");
    assert!(!rig.driver.session.bots.similar_bots.contains_key(&42));
    assert!(rig.driver.fetch_similar_bots(42).unwrap().is_some());
}

fn chosen(rig: &Rig, message: i64) -> Vec<bool> {
    let history = rig.driver.session.histories.get(&7).expect("history");
    let Some(MessageContent::Poll(poll)) = history.messages.get(&message).map(|m| &m.content)
    else {
        panic!("poll");
    };
    poll.poll.options.iter().map(|o| o.is_chosen).collect()
}

#[test]
fn a_refused_poll_vote_takes_the_mark_back() {
    let mut rig = Rig::new();
    rig.ingest(POLL_OPEN_JSON);
    let vote = rig
        .driver
        .send_poll_answer(ChatId(7), MessageId(106), 0)
        .unwrap();
    assert_eq!(chosen(&rig, 106), vec![true, false], "shown at once");
    // Closed meanwhile: TDLib refuses and sends no `updatePoll`.
    rig.refuse(vote, "MESSAGE_POLL_CLOSED");
    assert_eq!(chosen(&rig, 106), vec![false, false]);
}

#[test]
fn a_refused_topic_mute_shows_the_topic_unmuted_again() {
    let mut rig = Rig::new();
    rig.ingest(
        r#"{"@type":"updateUser","user":{"id":41,"first_name":"Bot","type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":true,"allows_users_to_create_topics":true,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#,
    );
    rig.ingest(
        r#"{"@type":"updateNewChat","chat":{"id":41,"title":"Bot","type":{"@type":"chatTypePrivate","user_id":41},"unread_count":0,"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"1","is_pinned":false}]}}"#,
    );
    let fetch = rig.sent("getForumTopics");
    let extra = fetch.last().expect("getForumTopics")["@extra"]
        .as_str()
        .unwrap()
        .to_string();
    rig.ingest(&format!(
        r#"{{"@type":"forumTopics","@extra":"{extra}","total_count":1,"topics":[{{"info":{{"@type":"forumTopicInfo","chat_id":41,"forum_topic_id":7,"name":"Trips","icon":{{"@type":"forumTopicIcon","color":0,"custom_emoji_id":"0"}},"is_general":false,"is_closed":false,"is_hidden":false}},"order":"5","is_pinned":false,"unread_count":0,"last_read_inbox_message_id":0}}],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}}"#
    ));
    let muted = |rig: &Rig| {
        rig.driver
            .session
            .ordered_forum_topics(ChatId(41))
            .first()
            .map(|topic| topic.notification_settings.is_muted())
    };
    assert_eq!(muted(&rig), Some(false));
    let asked = rig
        .driver
        .set_forum_topic_muted(ChatId(41), 7, true)
        .unwrap()
        .unwrap();
    assert_eq!(muted(&rig), Some(true), "shown at once");
    rig.refuse(asked, "TOPIC_ID_INVALID");
    assert_eq!(muted(&rig), Some(false));
}
