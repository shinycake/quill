//! Connect-driver tests: folder edits start from the folder's current
//! spec, "Add folder" answers are matched to their link, and chat-list
//! paging resumes after a failed page.
use super::super::super::*;
use super::super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{ChatId, RequestId};
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
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

const FOLDERS: &str = r#"{"@type":"updateChatFolders","chat_folders":[{"@type":"chatFolderInfo","id":5,"name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"Work","entities":[]}},"icon":null,"color_id":-1,"is_shareable":false}],"main_chat_list_position":0,"are_tags_enabled":false}"#;

impl Rig {
    /// Signed in, with private chats 7, 8 and 9 and folder 5.
    fn new() -> Self {
        let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let driver = ready_driver(&recorder, prepared, &sink, &seq);
        let mut rig = Self {
            driver,
            recorder,
            seq,
            sink,
            _dir: dir,
        };
        for id in [7, 8, 9] {
            rig.ingest(&format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"C{id}","type":{{"@type":"chatTypePrivate","user_id":{id}}},"unread_count":0}}}}"#
            ));
        }
        rig.ingest(FOLDERS);
        rig
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

    fn extra_of(value: &Value) -> RequestId {
        RequestId(value["@extra"].as_str().unwrap().parse().unwrap())
    }

    /// Answer the last `getChatFolder` with a folder holding `included`.
    fn answer_spec(&mut self, included: &[i64]) {
        let fetch = self.sent("getChatFolder");
        let extra = Self::extra_of(fetch.last().expect("getChatFolder sent"));
        let ids: Vec<String> = included.iter().map(i64::to_string).collect();
        self.ingest(&format!(
            r#"{{"@type":"chatFolder","@extra":"{}","name":{{"@type":"chatFolderName","text":{{"@type":"formattedText","text":"Work","entities":[]}}}},"icon":null,"color_id":-1,"is_shareable":false,"pinned_chat_ids":[],"included_chat_ids":[{}],"excluded_chat_ids":[],"exclude_muted":false,"exclude_read":false,"exclude_archived":false,"include_contacts":false,"include_non_contacts":false,"include_bots":false,"include_groups":false,"include_channels":false}}"#,
            extra.0,
            ids.join(",")
        ));
    }

    fn last_edit_included(&self) -> Value {
        self.sent("editChatFolder").last().expect("editChatFolder")["folder"]["included_chat_ids"]
            .clone()
    }
}

#[test]
fn a_second_removal_waits_for_the_first_edit_and_starts_from_the_new_spec() {
    let mut rig = Rig::new();
    rig.driver.remove_chat_from_folder(ChatId(8), 5).unwrap();
    rig.answer_spec(&[7, 8, 9]);
    assert_eq!(rig.last_edit_included(), serde_json::json!([7, 9]));

    // Removing 9 while that edit is out sends nothing yet.
    rig.driver.remove_chat_from_folder(ChatId(9), 5).unwrap();
    assert_eq!(rig.sent("editChatFolder").len(), 1);
    assert_eq!(rig.sent("getChatFolder").len(), 1);
    // The edit's answer: the folder's spec is fetched again.
    let edit = Rig::extra_of(rig.sent("editChatFolder").last().unwrap());
    rig.ingest(&format!(
        r#"{{"@type":"chatFolderInfo","@extra":"{}","id":5,"name":{{"@type":"chatFolderName","text":{{"@type":"formattedText","text":"Work","entities":[]}}}},"icon":null,"color_id":-1,"is_shareable":false}}"#,
        edit.0
    ));
    assert_eq!(rig.sent("getChatFolder").len(), 2);
    rig.answer_spec(&[7, 9]);
    assert_eq!(
        rig.last_edit_included(),
        serde_json::json!([7]),
        "8 is not written back"
    );
}

#[test]
fn a_folder_changed_since_its_spec_was_fetched_is_fetched_again() {
    let mut rig = Rig::new();
    rig.driver.fetch_chat_folder(5).unwrap();
    rig.answer_spec(&[7]);
    assert!(rig.driver.session.chat_list.fresh_spec(5).is_some());
    // Chat 8 joins the folder (here or on a phone): TDLib says so.
    rig.ingest(FOLDERS);
    assert!(rig.driver.session.chat_list.fresh_spec(5).is_none());
    assert!(
        rig.driver.session.chat_list.folder_specs.contains_key(&5),
        "kept for display"
    );

    rig.driver.remove_chat_from_folder(ChatId(7), 5).unwrap();
    assert!(rig.sent("editChatFolder").is_empty());
    assert_eq!(rig.sent("getChatFolder").len(), 2);
    rig.answer_spec(&[7, 8]);
    assert_eq!(rig.last_edit_included(), serde_json::json!([8]));
}

#[test]
fn a_failed_spec_fetch_drops_the_removal_instead_of_asking_again() {
    let mut rig = Rig::new();
    rig.driver.remove_chat_from_folder(ChatId(7), 5).unwrap();
    let fetch = Rig::extra_of(rig.sent("getChatFolder").last().unwrap());
    rig.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_FOLDER_NOT_FOUND"}}"#,
        fetch.0
    ));
    assert!(rig.driver.session.chat_list.folder_remove_queue.is_empty());
    assert_eq!(rig.sent("getChatFolder").len(), 1, "no request loop");
    assert!(rig.driver.session.chats_state.chat_action_error.is_some());
}

#[test]
fn an_add_folder_answer_for_an_earlier_link_is_dropped() {
    let mut rig = Rig::new();
    let first = rig
        .driver
        .check_folder_invite_link("https://t.me/addlist/aaa")
        .unwrap();
    let second = rig
        .driver
        .check_folder_invite_link("https://t.me/addlist/bbb")
        .unwrap();
    let info = |extra: RequestId, missing: i64| {
        format!(
            r#"{{"@type":"chatFolderInviteLinkInfo","@extra":"{}","chat_folder_info":{{"@type":"chatFolderInfo","id":0,"name":{{"@type":"chatFolderName","text":{{"@type":"formattedText","text":"Shared","entities":[]}}}},"icon":null,"color_id":-1,"is_shareable":true,"has_my_invite_links":false}},"missing_chat_ids":[{missing}],"added_chat_ids":[]}}"#,
            extra.0
        )
    };
    rig.ingest(&info(first, 41));
    assert!(rig.driver.session.chat_list.folder_invite_info.is_none());
    rig.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"INVITE_SLUG_EXPIRED"}}"#,
        first.0
    ));
    assert!(rig.driver.session.chat_list.folder_invite_error.is_none());
    rig.ingest(&info(second, 42));
    let shown = rig.driver.session.chat_list.folder_invite_info.as_ref();
    assert_eq!(shown.map(|i| i.missing_chat_ids.clone()), Some(vec![42]));

    // Confirm for this link, then another link opens before the answer:
    // the join does not say "done" in the new link's dialog.
    let add = rig
        .driver
        .add_folder_by_invite_link("https://t.me/addlist/bbb", &[42])
        .unwrap();
    let third = rig
        .driver
        .check_folder_invite_link("https://t.me/addlist/ccc")
        .unwrap();
    rig.ingest(&format!(r#"{{"@type":"ok","@extra":"{}"}}"#, add.0));
    assert!(!rig.driver.session.chat_list.folder_invite_done);
    rig.ingest(&info(third, 43));
    let add = rig
        .driver
        .add_folder_by_invite_link("https://t.me/addlist/ccc", &[43])
        .unwrap();
    rig.ingest(&format!(r#"{{"@type":"ok","@extra":"{}"}}"#, add.0));
    assert!(rig.driver.session.chat_list.folder_invite_done);
}

#[test]
fn main_list_paging_resumes_after_a_failed_page_and_the_archive_starts() {
    let mut rig = Rig::new();
    let page = Rig::extra_of(rig.sent("loadChats").last().expect("first page on Ready"));
    rig.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":500,"message":"Request aborted"}}"#,
        page.0
    ));
    // The archive does not wait for the main list's end.
    let archive = rig
        .sent("loadChats")
        .into_iter()
        .filter(|v| v["chat_list"]["@type"] == "chatListArchive")
        .count();
    assert_eq!(archive, 1);
    let main = |rig: &Rig| {
        rig.sent("loadChats")
            .into_iter()
            .filter(|v| v["chat_list"]["@type"] == "chatListMain")
            .count()
    };
    assert_eq!(main(&rig), 1);
    // Before the wait is over nothing goes out; after it, the page again.
    let now = std::time::Instant::now();
    let (at, _) = rig.driver.session.chat_list.chats_retry.expect("waiting");
    assert!(!rig.driver.chat_list_tick(now.min(at)));
    assert_eq!(main(&rig), 1);
    assert!(rig.driver.chat_list_tick(at));
    assert_eq!(main(&rig), 2);
    // A page that goes through ends the wait.
    let page = Rig::extra_of(
        rig.sent("loadChats")
            .into_iter()
            .rev()
            .find(|v| v["chat_list"]["@type"] == "chatListMain")
            .as_ref()
            .unwrap(),
    );
    rig.ingest(&format!(r#"{{"@type":"ok","@extra":"{}"}}"#, page.0));
    assert!(rig.driver.session.chat_list.chats_retry.is_none());
}
