//! Connect-driver tests (recorded TDLib JSON, nothing live): "Upload Sound"
//! and deleting a saved sound.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use crate::tone_upload::{ToneProblem, ToneUpload};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

type Recorder = Arc<RecordingSender>;

fn driver() -> (
    ConnectDriver<Recorder>,
    Recorder,
    Arc<dyn DiagnosticSink>,
    AtomicU64,
) {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink;
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    (driver, recorder, dyn_sink, seq)
}

fn tone_file(name: &str, size: usize) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("quill-tone-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, vec![0u8; size]).unwrap();
    path
}

fn answer(
    driver: &mut ConnectDriver<Recorder>,
    recorder: &RecordingSender,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
    type_name: &str,
    body: &str,
) {
    let request = sent_request(recorder, type_name);
    let extra = request["@extra"].as_str().unwrap().to_string();
    let mut value: Value = serde_json::from_str(body).unwrap();
    value["@extra"] = Value::String(extra);
    driver
        .ingest(copy_and_parse(&value.to_string(), seq, sink).unwrap())
        .unwrap();
}

const NEW_SOUND: &str = r#"{"@type":"notificationSound","id":"900","duration":3,"date":1,"title":"Soft Bell","data":"","sound":{"@type":"file","id":55,"size":4000,"expected_size":4000,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"y","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":4000}}}"#;

#[test]
fn a_picked_file_goes_up_as_a_local_input_file() {
    let (mut driver, recorder, _sink, _seq) = driver();
    let path = tone_file("Soft Bell.mp3", 4000);
    driver.upload_notification_tone(&path).unwrap();
    let request = sent_request(&recorder, "addSavedNotificationSound");
    assert_eq!(request["sound"]["@type"], "inputFileLocal");
    assert_eq!(request["sound"]["path"], path.to_str().unwrap());
    assert_eq!(
        driver.session.settings.tone_upload,
        ToneUpload::Uploading {
            name: "Soft Bell".into()
        }
    );
    assert_eq!(
        driver.upload_notification_tone(&path),
        Err(ToneProblem::Unreadable),
        "one file at a time"
    );
}

#[test]
fn a_file_over_the_option_limit_is_not_sent() {
    let (mut driver, recorder, _sink, _seq) = driver();
    driver.session.messages.tone_limits.max_size = 1000;
    let path = tone_file("Big.mp3", 1001);
    assert_eq!(
        driver.upload_notification_tone(&path),
        Err(ToneProblem::TooBig)
    );
    assert!(
        !recorder
            .snapshot()
            .iter()
            .any(|json| json.contains("addSavedNotificationSound"))
    );
    assert_eq!(driver.session.settings.tone_upload, ToneUpload::Idle);
}

#[test]
fn a_file_that_is_not_an_mp3_is_not_sent() {
    let (mut driver, _recorder, _sink, _seq) = driver();
    let path = tone_file("notes.txt", 10);
    assert_eq!(
        driver.upload_notification_tone(&path),
        Err(ToneProblem::InvalidFile)
    );
}

#[test]
fn the_new_sound_joins_the_list_and_is_reported_once() {
    let (mut driver, recorder, sink, seq) = driver();
    let path = tone_file("Soft Bell.mp3", 4000);
    driver.upload_notification_tone(&path).unwrap();
    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "addSavedNotificationSound",
        NEW_SOUND,
    );
    let ids: Vec<i64> = driver
        .session
        .settings
        .saved_notification_sounds
        .iter()
        .map(|sound| sound.id)
        .collect();
    assert_eq!(ids, vec![900]);
    assert_eq!(
        driver.take_tone_upload_outcome(),
        Some(ToneUpload::Added { sound_id: 900 })
    );
    assert_eq!(driver.take_tone_upload_outcome(), None);
}

#[test]
fn the_server_refusing_the_duration_is_reported() {
    let (mut driver, recorder, sink, seq) = driver();
    let path = tone_file("Long.mp3", 4000);
    driver.upload_notification_tone(&path).unwrap();
    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "addSavedNotificationSound",
        r#"{"@type":"error","code":400,"message":"RINGTONE_DURATION_TOO_LONG"}"#,
    );
    assert_eq!(
        driver.take_tone_upload_outcome(),
        Some(ToneUpload::Failed(ToneProblem::TooLong))
    );
}

#[test]
fn deleting_a_saved_sound_sends_its_id_and_refetches() {
    let (mut driver, recorder, sink, seq) = driver();
    driver.remove_notification_tone(900).unwrap();
    let request = sent_request(&recorder, "removeSavedNotificationSound");
    assert_eq!(request["notification_sound_id"], "900");
    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "removeSavedNotificationSound",
        r#"{"@type":"ok"}"#,
    );
    assert!(driver.session.settings.saved_sounds_stale);
    assert!(driver.remove_notification_tone(0).is_err());
}
