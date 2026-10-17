//! Connect-driver tests: the story viewer's preloads. A preloaded story
//! that becomes current asks for its whole file at the user priority, and
//! preloads the viewer no longer wants are cancelled, as tdesktop's
//! `StoryPreload` cancels when it is replaced or the viewer closes.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, FileId};
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::story_preload::PreloadRequest;
use crate::telegram::client::copy_and_parse;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const PREFIX: i64 = 4 * 1024 * 1024;

struct Harness {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    sink: Arc<dyn DiagnosticSink>,
    seq: AtomicU64,
    dir: std::path::PathBuf,
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Harness {
    fn new(store: &MemorySecretStore) -> Self {
        let (dir, prepared) = prepared_tmp(store);
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), sink.clone());
        let driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let mut harness = Self {
            driver,
            recorder,
            sink,
            seq: AtomicU64::new(0),
            dir,
        };
        harness.ingest(&json!({"@type": "updateAuthorizationState",
                "authorization_state": {"@type": "authorizationStateReady"}}));
        harness
    }

    fn ingest(&mut self, value: &Value) {
        let owned = copy_and_parse(&value.to_string(), &self.seq, &self.sink).expect("parses");
        self.driver.ingest(owned).expect("ingests");
    }

    /// TDLib's word on a file: `downloaded` bytes, transfer `active` or not.
    fn file(&mut self, id: i32, downloaded: i64, active: bool) {
        self.ingest(&json!({"@type": "updateFile", "file": {"@type": "file", "id": id,
            "size": 9_000_000, "expected_size": 9_000_000,
            "local": {"@type": "localFile", "path": "", "can_be_downloaded": true,
                "can_be_deleted": false, "is_downloading_active": active,
                "is_downloading_completed": false, "download_offset": 0,
                "downloaded_prefix_size": downloaded, "downloaded_size": downloaded},
            "remote": {"@type": "remoteFile", "id": format!("r{id}"), "unique_id": format!("u{id}"),
                "is_uploading_active": false, "is_uploading_completed": true, "uploaded_size": 0}}}));
    }

    fn sent(&self, kind: &str, file: i32) -> Vec<Value> {
        self.recorder
            .snapshot()
            .into_iter()
            .map(|json| serde_json::from_str::<Value>(&json).expect("json"))
            .filter(|v| v["@type"] == kind && v["file_id"] == file)
            .collect()
    }
}

fn preload(file: i32, priority: i32, limit: i64) -> PreloadRequest {
    PreloadRequest {
        file_id: FileId(file),
        priority,
        limit,
    }
}

fn whole_file_at_user_priority(request: &Value) -> bool {
    request["limit"] == 0 && request["priority"] == USER_DOWNLOAD_PRIORITY
}

/// Regression: the prefix request was marked in flight, so stepping to
/// the story sent nothing more; TDLib stopped at the prefix and the clip
/// never played.
#[test]
fn a_preload_in_flight_is_asked_for_in_full_when_its_story_becomes_current() {
    let store = MemorySecretStore::new();
    let mut h = Harness::new(&store);
    h.file(31, 0, false);
    h.driver
        .sync_story_preloads(&[preload(31, 7, PREFIX)], &[])
        .expect("sent");
    let first = h.sent("downloadFile", 31);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0]["limit"], PREFIX);
    assert_eq!(first[0]["priority"], 7);
    h.file(31, 1_000_000, true);

    h.driver
        .download_user_file(FileId(31), None)
        .expect("sent")
        .expect("a request for the whole file");
    let requests = h.sent("downloadFile", 31);
    assert_eq!(requests.len(), 2);
    assert!(whole_file_at_user_priority(&requests[1]), "{}", requests[1]);
    // Now current: the next plan neither re-preloads nor cancels it.
    h.driver
        .sync_story_preloads(&[], &[FileId(31)])
        .expect("sent");
    h.driver.sync_story_preloads(&[], &[]).expect("sent");
    assert!(h.sent("cancelDownloadFile", 31).is_empty());
}

#[test]
fn a_finished_prefix_is_not_resent_and_still_upgrades_when_current() {
    let store = MemorySecretStore::new();
    let mut h = Harness::new(&store);
    h.file(31, 0, false);
    h.driver
        .sync_story_preloads(&[preload(31, 7, PREFIX)], &[])
        .expect("sent");
    h.file(31, PREFIX, true);
    // The prefix is in: TDLib stops the transfer short of the whole file.
    h.file(31, PREFIX, false);
    h.driver
        .sync_story_preloads(&[preload(31, 7, PREFIX)], &[])
        .expect("sent");
    assert_eq!(h.sent("downloadFile", 31).len(), 1, "no preload loop");

    h.driver
        .download_user_file(FileId(31), None)
        .expect("sent")
        .expect("a request for the whole file");
    let requests = h.sent("downloadFile", 31);
    assert_eq!(requests.len(), 2);
    assert!(whole_file_at_user_priority(&requests[1]), "{}", requests[1]);
}

#[test]
fn preloads_the_viewer_drops_are_cancelled_and_closing_cancels_the_rest() {
    let store = MemorySecretStore::new();
    let mut h = Harness::new(&store);
    for id in [40, 41, 42] {
        h.file(id, 0, false);
    }
    h.driver
        .sync_story_preloads(
            &[preload(40, 7, 0), preload(41, 6, 0), preload(42, 5, 0)],
            &[],
        )
        .expect("sent");

    // One step on: 40 is the current story now, 41 still next, 42 gone.
    h.driver
        .sync_story_preloads(&[preload(41, 7, 0)], &[FileId(40)])
        .expect("sent");
    let cancels = h.sent("cancelDownloadFile", 42);
    assert_eq!(cancels.len(), 1);
    assert_eq!(cancels[0]["only_if_pending"], false);
    assert!(h.sent("cancelDownloadFile", 40).is_empty(), "current kept");
    assert!(h.sent("cancelDownloadFile", 41).is_empty(), "still planned");
    assert_eq!(h.sent("downloadFile", 41).len(), 1, "not asked twice");

    // The viewer closes.
    h.driver.sync_story_preloads(&[], &[]).expect("sent");
    assert_eq!(h.sent("cancelDownloadFile", 40).len(), 1);
    assert_eq!(h.sent("cancelDownloadFile", 41).len(), 1);
    assert_eq!(h.sent("cancelDownloadFile", 42).len(), 1, "once");
}

/// Regression: an automatic download that found the preload running sent
/// nothing, so the preload's prefix limit stayed and dropping the preload
/// cancelled the file the automatic path wanted.
#[test]
fn an_automatic_download_over_a_preload_takes_it_over() {
    let store = MemorySecretStore::new();
    let mut h = Harness::new(&store);
    h.file(31, 0, false);
    h.driver
        .sync_story_preloads(&[preload(31, 7, PREFIX)], &[])
        .expect("sent");
    h.file(31, 1_000_000, true);
    h.driver
        .download_file(FileId(31), 5)
        .expect("sent")
        .expect("a request for the whole file");
    let requests = h.sent("downloadFile", 31);
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1]["limit"], 0);
    assert_eq!(requests[1]["priority"], 5);
    h.driver.sync_story_preloads(&[], &[]).expect("sent");
    assert!(
        h.sent("cancelDownloadFile", 31).is_empty(),
        "no longer a preload"
    );
    h.driver.download_file(FileId(31), 5).expect("sent");
    assert_eq!(h.sent("downloadFile", 31).len(), 2, "asked once");
}
