//! Connect-driver tests: the profile photo viewer's requests for groups,
//! channels and users (TDLib 1.8.69 JSON shapes).
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{ChatId, MessageId};
use crate::platform::MemorySecretStore;
use crate::state::{PhotoDelete, PhotoOwner};
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const GROUP: i64 = -1009;

struct Fixture {
    dir: std::path::PathBuf,
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    sink: Arc<dyn DiagnosticSink>,
    seq: AtomicU64,
}

impl Fixture {
    fn new() -> Self {
        let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let driver = ready_driver(&recorder, prepared, &sink, &seq);
        Self {
            dir,
            driver,
            recorder,
            sink,
            seq,
        }
    }

    fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    fn sent(&self) -> Vec<Value> {
        self.recorder
            .snapshot()
            .iter()
            .map(|json| serde_json::from_str(json).unwrap())
            .collect()
    }

    fn sent_of(&self, kind: &str) -> Vec<Value> {
        self.sent()
            .into_iter()
            .filter(|v| v["@type"] == kind)
            .collect()
    }

    fn seed_group(&mut self, status: &str) {
        self.ingest(&format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{GROUP},"title":"Group","type":{{"@type":"chatTypeSupergroup","supergroup_id":9,"is_channel":false}},"unread_count":0}}}}"#
        ));
        self.ingest(&format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":9,"is_forum":false,"status":{status}}}}}"#
        ));
    }

    fn answer_history(&mut self, request: &Value, next: i64, messages: &str) {
        let extra = request["@extra"].as_str().unwrap().to_string();
        self.ingest(&format!(
            r#"{{"@type":"foundChatMessages","@extra":"{extra}","total_count":250,"next_from_message_id":{next},"messages":[{messages}]}}"#
        ));
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn change_json(message_id: i64) -> String {
    format!(
        r#"{{"@type":"message","id":{message_id},"chat_id":{GROUP},"is_outgoing":false,"date":1,"content":{{"@type":"messageChatChangePhoto","photo":{{"@type":"chatPhoto","id":"{message_id}","added_date":1,"sizes":[{{"@type":"photoSize","type":"x","photo":{},"width":640,"height":640,"progressive_sizes":[]}}]}}}}}}"#,
        file_json(message_id as i32)
    )
}

fn file_json(id: i32) -> String {
    format!(
        r#"{{"@type":"file","id":{id},"size":10,"expected_size":10,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":10}}}}"#
    )
}

const MEMBER: &str = r#"{"@type":"chatMemberStatusMember"}"#;
const CREATOR: &str = r#"{"@type":"chatMemberStatusCreator","is_anonymous":false,"is_member":true}"#;

#[test]
fn a_group_viewer_asks_for_the_current_photo_and_the_photo_history() {
    let mut f = Fixture::new();
    f.seed_group(MEMBER);
    f.driver.fetch_chat_photos(ChatId(GROUP)).unwrap();
    let full = f.sent_of("getSupergroupFullInfo");
    assert_eq!(full.len(), 1);
    assert_eq!(full.first().map(|v| v["supergroup_id"].clone()), Some(9.into()));
    let search = f.sent_of("searchChatMessages");
    let Some(search) = search.first() else {
        panic!("history asked for");
    };
    assert_eq!(search["chat_id"], GROUP);
    assert_eq!(search["filter"]["@type"], "searchMessagesFilterChatPhoto");
    assert_eq!(search["from_message_id"], 0);
    assert_eq!(search["limit"], 100);
    // In flight: nothing is sent twice.
    f.driver.fetch_chat_photos(ChatId(GROUP)).unwrap();
    assert_eq!(f.sent_of("searchChatMessages").len(), 1);
    assert_eq!(f.sent_of("getSupergroupFullInfo").len(), 1);
}

#[test]
fn later_history_pages_start_from_the_last_message() {
    let mut f = Fixture::new();
    f.seed_group(MEMBER);
    f.driver.fetch_chat_photos(ChatId(GROUP)).unwrap();
    let first = f.sent_of("searchChatMessages").remove(0);
    f.answer_history(&first, 40, &change_json(40));
    let page = f
        .driver
        .fetch_more_photos(PhotoOwner::Chat(ChatId(GROUP)))
        .unwrap();
    assert!(page.is_some());
    let searches = f.sent_of("searchChatMessages");
    assert_eq!(searches.get(1).map(|v| v["from_message_id"].clone()), Some(40.into()));
    // One page at a time.
    assert!(
        f.driver
            .fetch_more_photos(PhotoOwner::Chat(ChatId(GROUP)))
            .unwrap()
            .is_none()
    );
    // An empty page ends the list.
    let second = searches.get(1).cloned().unwrap();
    f.answer_history(&second, 0, "");
    assert!(
        f.driver
            .fetch_more_photos(PhotoOwner::Chat(ChatId(GROUP)))
            .unwrap()
            .is_none()
    );
}

#[test]
fn user_photos_page_by_offset() {
    let mut f = Fixture::new();
    let extra = f.driver.fetch_user_profile_photos(31).unwrap().unwrap();
    f.ingest(&format!(
        r#"{{"@type":"chatPhotos","@extra":"{}","total_count":150,"photos":[{{"@type":"chatPhoto","id":"1","added_date":1,"sizes":[{{"@type":"photoSize","type":"x","photo":{},"width":640,"height":640,"progressive_sizes":[]}}]}}]}}"#,
        extra.0,
        file_json(5)
    ));
    f.driver.fetch_more_photos(PhotoOwner::User(31)).unwrap();
    let pages = f.sent_of("getUserProfilePhotos");
    assert_eq!(pages.get(1).map(|v| v["offset"].clone()), Some(1.into()));
    assert_eq!(pages.get(1).map(|v| v["user_id"].clone()), Some(31.into()));
}

#[test]
fn set_main_and_delete_send_the_chat_requests() {
    let mut f = Fixture::new();
    f.seed_group(MEMBER);
    // A member without the right may not set the group's photo.
    assert!(f.driver.set_chat_photo_from_earlier(ChatId(GROUP), 77).is_err());
    f.seed_group(CREATOR);
    f.driver.set_chat_photo_from_earlier(ChatId(GROUP), 77).unwrap();
    let set = f.sent_of("setChatPhoto");
    let Some(set) = set.first() else {
        panic!("setChatPhoto sent");
    };
    assert_eq!(set["photo"]["@type"], "inputChatPhotoStatic");
    assert_eq!(set["photo"]["photo"]["@type"], "inputFileId");
    assert_eq!(set["photo"]["photo"]["id"], 77);

    // The earlier photo's change message is deleted once it is listed.
    let delete = PhotoDelete::Message {
        chat_id: ChatId(GROUP),
        message_id: MessageId(40),
        is_outgoing: false,
    };
    assert!(f.driver.delete_photo(delete, true).is_err());
    f.driver.fetch_chat_photos(ChatId(GROUP)).unwrap();
    let search = f.sent_of("searchChatMessages").remove(0);
    f.answer_history(&search, 0, &change_json(40));
    f.driver.delete_photo(delete, true).unwrap();
    let deleted = f.sent_of("deleteMessages");
    assert_eq!(deleted.first().map(|v| v["message_ids"].clone()), Some(serde_json::json!([40])));
    assert_eq!(deleted.first().map(|v| v["revoke"].clone()), Some(true.into()));
}

#[test]
fn reports_name_the_chat_and_never_yourself() {
    let mut f = Fixture::new();
    f.driver.session.my_user_id = Some(777);
    assert!(f.driver.report_photo(PhotoOwner::User(777), 5, "reportReasonSpam").is_err());
    f.driver
        .report_photo(PhotoOwner::User(31), 5, "reportReasonSpam")
        .unwrap();
    f.driver
        .report_photo(PhotoOwner::Chat(ChatId(GROUP)), 6, "reportReasonViolence")
        .unwrap();
    let reports = f.sent_of("reportChatPhoto");
    assert_eq!(reports.first().map(|v| v["chat_id"].clone()), Some(31.into()));
    assert_eq!(reports.get(1).map(|v| v["chat_id"].clone()), Some(GROUP.into()));
    assert_eq!(reports.get(1).map(|v| v["file_id"].clone()), Some(6.into()));
}
