//! Connect-driver tests: saved music (check, add, remove, reorder).
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::FileId;
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const ME: i64 = 31;

fn last_request(recorder: &RecordingSender) -> serde_json::Value {
    serde_json::from_str(recorder.snapshot().last().expect("a request")).unwrap()
}

fn answer(
    driver: &mut ConnectDriver<Arc<RecordingSender>>,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
    request: &serde_json::Value,
    body: &str,
) {
    let json = format!(
        r#"{{{body},"@extra":"{}"}}"#,
        request["@extra"].as_str().unwrap()
    );
    driver
        .ingest(copy_and_parse(&json, seq, sink).unwrap())
        .unwrap();
}

fn track(id: i32) -> String {
    format!(
        r#"{{"@type":"audio","duration":5,"title":"T{id}","performer":"P","file_name":"{id}.mp3","mime_type":"audio/mpeg","audio":{{"@type":"file","id":{id},"size":1,"expected_size":1,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"r{id}","unique_id":"u{id}","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":1}}}}}}"#
    )
}

type Setup = (
    ConnectDriver<Arc<RecordingSender>>,
    Arc<RecordingSender>,
    AtomicU64,
    Arc<dyn DiagnosticSink>,
    std::path::PathBuf,
);

fn driver_with_playlist(ids: &[i32]) -> Setup {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver.session.my_user_id = Some(ME);
    driver.fetch_playlist(ME, false).unwrap();
    let request = last_request(&recorder);
    let tracks: Vec<String> = ids.iter().map(|id| track(*id)).collect();
    let body = format!(
        r#""@type":"audios","total_count":{},"audios":[{}]"#,
        ids.len(),
        tracks.join(",")
    );
    answer(&mut driver, &seq, &sink, &request, &body);
    (driver, recorder, seq, sink, dir)
}

fn order(driver: &ConnectDriver<Arc<RecordingSender>>) -> Vec<i32> {
    driver.session.media.playlists[&ME]
        .tracks
        .iter()
        .map(|t| t.file_id.0)
        .collect()
}

#[test]
fn a_song_is_checked_once_and_a_404_means_not_saved() {
    let (mut driver, recorder, seq, sink, dir) = driver_with_playlist(&[1]);
    driver.check_profile_audio(FileId(9)).unwrap();
    driver.check_profile_audio(FileId(9)).unwrap();
    let request = last_request(&recorder);
    assert_eq!(request["@type"], "isProfileAudio");
    assert_eq!(request["file_id"], 9);
    assert_eq!(
        recorder
            .snapshot()
            .iter()
            .filter(|json| json.contains("isProfileAudio"))
            .count(),
        1
    );
    assert_eq!(driver.session.profile_audio_known(FileId(9)), None);
    answer(
        &mut driver,
        &seq,
        &sink,
        &request,
        r#""@type":"error","code":404,"message":"Not Found""#,
    );
    assert_eq!(driver.session.profile_audio_known(FileId(9)), Some(false));
    // A saved song answers Ok.
    driver.check_profile_audio(FileId(10)).unwrap();
    let request = last_request(&recorder);
    answer(&mut driver, &seq, &sink, &request, r#""@type":"ok""#);
    assert_eq!(driver.session.profile_audio_known(FileId(10)), Some(true));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn saving_a_song_puts_it_first_and_a_failure_puts_the_list_back() {
    let (mut driver, recorder, seq, sink, dir) = driver_with_playlist(&[1, 2]);
    driver
        .save_audio_to_profile(FileId(9), 120, "Song", "Artist", "song.mp3")
        .unwrap();
    let request = last_request(&recorder);
    assert_eq!(request["@type"], "addProfileAudio");
    assert_eq!(request["audio"]["audio"]["id"], 9);
    assert_eq!(order(&driver), [9, 1, 2]);
    assert_eq!(driver.session.profile_audio_known(FileId(9)), Some(true));
    // Saving it again is refused.
    assert!(
        driver
            .save_audio_to_profile(FileId(9), 120, "Song", "Artist", "song.mp3")
            .is_err()
    );
    let before = recorder.snapshot().len();
    answer(
        &mut driver,
        &seq,
        &sink,
        &request,
        r#""@type":"error","code":400,"message":"AUDIO_INVALID""#,
    );
    assert_eq!(order(&driver), [1, 2]);
    assert_eq!(driver.session.profile_audio_known(FileId(9)), None);
    // The failed change reloads the playlist (the driver does it while it
    // ingests the answer).
    assert!(recorder.snapshot().len() > before);
    assert_eq!(last_request(&recorder)["@type"], "getUserProfileAudios");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn removing_and_moving_tracks_update_the_list_and_tell_telegram() {
    let (mut driver, recorder, _seq, _sink, dir) = driver_with_playlist(&[1, 2, 3]);
    driver.move_profile_audio_track(0, 2).unwrap();
    let request = last_request(&recorder);
    assert_eq!(request["@type"], "setProfileAudioPosition");
    assert_eq!(request["file_id"], 1);
    assert_eq!(request["after_file_id"], 3);
    assert_eq!(order(&driver), [2, 3, 1]);
    // To the front: after_file_id 0.
    driver.move_profile_audio_track(2, 0).unwrap();
    let request = last_request(&recorder);
    assert_eq!(request["file_id"], 1);
    assert_eq!(request["after_file_id"], 0);
    // The same position sends nothing.
    let sent = recorder.snapshot().len();
    driver.move_profile_audio_track(1, 1).unwrap();
    assert_eq!(recorder.snapshot().len(), sent);
    driver.remove_profile_audio_track(FileId(3)).unwrap();
    let request = last_request(&recorder);
    assert_eq!(request["@type"], "removeProfileAudio");
    assert_eq!(request["file_id"], 3);
    assert_eq!(order(&driver), [1, 2]);
    assert_eq!(driver.session.media.playlists[&ME].total_count, 2);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_track_of_someone_elses_playlist_goes_to_saved_messages_by_file_id() {
    let (mut driver, recorder, _seq, _sink, dir) = driver_with_playlist(&[4]);
    driver.send_track_to_saved_messages(FileId(4)).unwrap();
    let request = last_request(&recorder);
    assert_eq!(request["@type"], "sendMessage");
    assert_eq!(request["chat_id"], ME);
    assert_eq!(request["input_message_content"]["audio"]["audio"]["id"], 4);
    assert!(driver.send_track_to_saved_messages(FileId(99)).is_err());
    let _ = std::fs::remove_dir_all(dir);
}
