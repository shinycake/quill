//! A recording goes to the topic it was recorded in, fixed at Send, not to
//! the topic or chat open when its file is ready.

use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{ChatId, TopicId};
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use crate::voice::VoiceDraft;
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
    /// Forum 16 with topics 3 and 5, open on topic 3; chat 7 beside it.
    fn forum() -> Self {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
        let mut f = Self {
            driver,
            recorder,
            dyn_sink,
            seq,
            dir,
        };
        f.ingest(r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Forum","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"unread_count":0}}"#);
        f.ingest(r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":16,"is_forum":true,"status":{"@type":"chatMemberStatusMember"}}}"#);
        f.ingest(r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#);
        f.driver.select_chat(ChatId(16)).unwrap();
        f.driver.select_topic(3).unwrap();
        f
    }

    fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.dyn_sink).unwrap())
            .unwrap();
    }

    fn draft(&self) -> VoiceDraft {
        let path = self.dir.join("note.ogg");
        std::fs::write(&path, b"OggS").unwrap();
        VoiceDraft {
            path,
            duration_secs: 3,
            bars: vec![31, 0, 1],
        }
    }

    fn last_voice(&self) -> Value {
        self.recorder
            .snapshot()
            .into_iter()
            .rev()
            .find(|json| json.contains("inputMessageVoiceNote"))
            .map(|json| serde_json::from_str(&json).unwrap())
            .expect("a voice note was sent")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn a_recording_goes_to_the_topic_it_was_sent_from() {
    let mut f = Fixture::forum();
    let place = f.driver.send_place(ChatId(16));
    assert_eq!(place.topic, Some(TopicId::forum(3)));
    // Topic 5 opened while the file was finished.
    f.driver.select_topic(5).unwrap();
    let draft = f.draft();
    f.driver
        .send_voice_note_to(ChatId(16), &place, &draft, "", None, false)
        .unwrap();
    let sent = f.last_voice();
    assert_eq!(sent["chat_id"], 16);
    assert_eq!(sent["topic_id"]["forum_topic_id"], 3);
}

#[test]
fn a_recording_keeps_its_topic_when_another_chat_opens() {
    let mut f = Fixture::forum();
    let place = f.driver.send_place(ChatId(16));
    f.driver.select_chat(ChatId(7)).unwrap();
    let draft = f.draft();
    f.driver
        .send_voice_note_to(ChatId(16), &place, &draft, "", None, false)
        .unwrap();
    let sent = f.last_voice();
    assert_eq!(sent["chat_id"], 16);
    assert_eq!(
        sent["topic_id"]["forum_topic_id"], 3,
        "not General, where every member would see it"
    );
}

#[test]
fn a_recording_for_a_closed_topic_is_refused_whatever_is_open() {
    let mut f = Fixture::forum();
    let extra = f
        .recorder
        .snapshot()
        .into_iter()
        .map(|json| serde_json::from_str::<Value>(&json).unwrap())
        .rfind(|v| v["@type"] == "getForumTopics")
        .expect("the forum's topics were asked for")["@extra"]
        .as_str()
        .unwrap()
        .to_owned();
    let topic = |id: i32, closed: bool| {
        format!(
            r#"{{"info":{{"@type":"forumTopicInfo","chat_id":16,"forum_topic_id":{id},"name":"T{id}","icon":{{"@type":"forumTopicIcon","color":0,"custom_emoji_id":"0"}},"is_general":false,"is_closed":{closed},"is_hidden":false}},"order":"{id}","is_pinned":false,"unread_count":0,"last_read_inbox_message_id":0,"notification_settings":{{"@type":"chatNotificationSettings","use_default_mute_for":true,"mute_for":0}}}}"#
        )
    };
    f.ingest(&format!(
        r#"{{"@type":"forumTopics","@extra":"{extra}","total_count":2,"topics":[{},{}],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}}"#,
        topic(5, false),
        topic(3, true)
    ));
    let closed = |f: &Fixture, id: i32| {
        f.driver
            .session
            .threads
            .forum_topics
            .get(&16)
            .and_then(|topics| topics.iter().find(|t| t.forum_topic_id == id))
            .map(|t| t.is_closed)
    };
    assert_eq!((closed(&f, 3), closed(&f, 5)), (Some(true), Some(false)));
    let place = f.driver.send_place(ChatId(16));
    f.driver.select_topic(5).unwrap();
    let draft = f.draft();
    assert_eq!(
        f.driver
            .send_voice_note_to(ChatId(16), &place, &draft, "", None, false),
        Err(ConnectSendError::InvalidRequest)
    );
    // The open topic is not the one checked: topic 5's own send goes out.
    let open = f.driver.send_place(ChatId(16));
    f.driver
        .send_voice_note_to(ChatId(16), &open, &draft, "", None, false)
        .unwrap();
    assert_eq!(f.last_voice()["topic_id"]["forum_topic_id"], 5);
}
