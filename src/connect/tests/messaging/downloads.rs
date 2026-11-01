//! Driver tests: automatic photo and chat-photo downloads.
use super::*;

#[test]
fn photo_history_auto_downloads_thumb_and_full_photo() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let history_extra = driver.select_chat(ChatId(7)).unwrap().expect("history");
    let thumb = r#"{"@type":"file","id":1,"size":10,"expected_size":10,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"CANARY_REMOTE","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":10}}"#;
    let full = r#"{"@type":"file","id":2,"size":80,"expected_size":80,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"CANARY_REMOTE","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":80}}"#;
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"messages","@extra":"{extra}","messages":[{{"id":20,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{thumb},"width":320,"height":240,"progressive_sizes":[]}},{{"@type":"photoSize","type":"x","photo":{full},"width":800,"height":600,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"CANARY_MEDIA","entities":[]}},"has_spoiler":false,"is_secret":false}}}}]}}"#,
                        extra = history_extra.0,
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let sent = recorder.snapshot();
    let thumb_req = sent
        .iter()
        .rev()
        .find(|j| j.contains("downloadFile") && j.contains("\"file_id\":1"))
        .expect("auto thumb downloadFile");
    let thumb_json: Value = serde_json::from_str(thumb_req).unwrap();
    assert_eq!(thumb_json["priority"], THUMB_DOWNLOAD_PRIORITY);
    assert_eq!(thumb_json["synchronous"], false);
    // MED3: the full photo auto-downloads too when the photo flag is on
    // (TGX auto-download), at the auto-media priority below explicit
    // user downloads.
    let full_req = sent
        .iter()
        .rev()
        .find(|j| j.contains("downloadFile") && j.contains("\"file_id\":2"))
        .expect("auto full-photo downloadFile");
    let full_json: Value = serde_json::from_str(full_req).unwrap();
    assert_eq!(full_json["priority"], AUTO_MEDIA_DOWNLOAD_PRIORITY);
    assert_eq!(full_json["synchronous"], false);
    // A user open while the auto download is in flight dedupes instead
    // of re-requesting.
    assert_eq!(
        driver.download_user_file(FileId(2), None),
        Ok(None),
        "in-flight download must not duplicate"
    );
    assert!(!sink.rendered().contains("CANARY_MEDIA"));
    assert!(!sink.rendered().contains("CANARY_REMOTE"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn chat_list_photo_downloads_on_ingest_and_dedupes() {
    // Parity slice: `updateNewChat` with `chat.photo.small` triggers a
    // `downloadFile` (below the open chat's thumbs) from the ingest hook; a
    // second ingest does not re-request while the download is in flight.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let small = r#"{"@type":"file","id":91,"size":24,"expected_size":24,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}"#;
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"updateNewChat","chat":{{"id":7,"title":"Ada","type":{{"@type":"chatTypePrivate","user_id":7}},"unread_count":0,"photo":{{"@type":"chatPhotoInfo","small":{small},"big":null,"minithumbnail":null,"has_animation":false,"is_personal":false}}}}}}"#
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let sent = recorder.snapshot();
    let avatar_req = sent
        .iter()
        .find(|j| j.contains("downloadFile") && j.contains("\"file_id\":91"))
        .expect("chat photo downloadFile");
    let avatar_json: Value = serde_json::from_str(avatar_req).unwrap();
    assert_eq!(avatar_json["priority"], CHAT_LIST_PHOTO_PRIORITY);
    const { assert!(CHAT_LIST_PHOTO_PRIORITY < THUMB_DOWNLOAD_PRIORITY) };
    assert_eq!(avatar_json["synchronous"], false);
    // A second ingest (any envelope) must not duplicate the in-flight
    // download.
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateChatTitle","chat_id":7,"title":"Ada"}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    let after = recorder.snapshot();
    let avatar_downloads = after
        .iter()
        .filter(|j| j.contains("downloadFile") && j.contains("\"file_id\":91"))
        .count();
    assert_eq!(avatar_downloads, 1, "in-flight avatar download deduped");
    driver.ingest(copy_and_parse(
        r#"{"@type":"updateUser","user":{"id":42,"first_name":"Contact","type":{"@type":"userTypeRegular"},"profile_photo":{"@type":"profilePhoto","small":{"@type":"file","id":92},"big":null}}}"#,
        &seq, &dyn_sink).unwrap()).unwrap();
    let user_downloads = |recorder: &RecordingSender| {
        recorder
            .snapshot()
            .iter()
            .filter(|j| j.contains("downloadFile") && j.contains("\"file_id\":92"))
            .cloned()
            .collect::<Vec<_>>()
    };
    let tick = |driver: &mut ConnectDriver<Arc<RecordingSender>>| {
        driver.poll_ticks(
            crate::connect::TickScope::OnScreen {
                settings_open: false,
            },
            std::time::Instant::now(),
            ONLINE,
        );
    };
    tick(&mut driver);
    assert!(
        user_downloads(&recorder).is_empty(),
        "a user no view shows is not downloaded"
    );
    assert_eq!(driver.session.user_photo_path(42), None);
    tick(&mut driver);
    let sent = user_downloads(&recorder);
    assert_eq!(sent.len(), 1, "the avatar a view asked for is downloaded");
    let user_json: Value = serde_json::from_str(&sent[0]).unwrap();
    assert_eq!(user_json["priority"], THUMB_DOWNLOAD_PRIORITY);
    assert_eq!(driver.session.user_photo_path(42), None);
    tick(&mut driver);
    assert_eq!(
        user_downloads(&recorder).len(),
        1,
        "claimed again while in flight"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn chat_list_photos_download_whenever_they_become_due() {
    // Avatars are tracked incrementally, so every way an avatar becomes
    // downloadable must still reach `downloadFile`: arriving before Ready,
    // a photo change while data saver is on (sent once it is off), and a
    // completed avatar evicted from TDLib's cache.
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    };
    let file = |id: i32, path: &str, completed: bool| {
        format!(
            r#"{{"@type":"file","id":{id},"size":24,"expected_size":24,"local":{{"@type":"localFile","path":"{path}","can_be_downloaded":true,"can_be_deleted":true,"is_downloading_active":false,"is_downloading_completed":{completed},"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"x","unique_id":"u{id}","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}}}"#
        )
    };
    let downloads = |recorder: &RecordingSender, id: i32| {
        recorder
            .snapshot()
            .iter()
            .filter(|j| {
                j.contains("\"@type\":\"downloadFile\"") && j.contains(&format!("\"file_id\":{id}"))
            })
            .count()
    };
    let tick = r#"{"@type":"updateChatTitle","chat_id":7,"title":"c"}"#;
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":7,"title":"c","type":{{"@type":"chatTypePrivate","user_id":7}},"unread_count":0,"photo":{{"@type":"chatPhotoInfo","small":{},"big":null,"minithumbnail":null,"has_animation":false,"is_personal":false}}}}}}"#,
            file(91, "", false)
        ),
    );
    assert_eq!(downloads(&recorder, 91), 0, "nothing before Ready");
    ingest(
        &mut driver,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
    );
    assert_eq!(downloads(&recorder, 91), 1, "pre-Ready avatar after Ready");

    driver.session.settings.media_prefs.data_saver = true;
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"updateChatPhoto","chat_id":7,"photo":{{"@type":"chatPhotoInfo","small":{},"big":null,"minithumbnail":null,"has_animation":false,"is_personal":false}}}}"#,
            file(93, "", false)
        ),
    );
    assert_eq!(downloads(&recorder, 93), 0, "data saver pauses avatars");
    driver.session.settings.media_prefs.data_saver = false;
    ingest(&mut driver, tick);
    assert_eq!(downloads(&recorder, 93), 1, "sent once data saver is off");

    let extra = recorder
        .snapshot()
        .iter()
        .filter_map(|j| serde_json::from_str::<Value>(j).ok())
        .find(|v| v["@type"] == "downloadFile" && v["file_id"] == 93)
        .and_then(|v| v["@extra"].as_str().map(str::to_string))
        .expect("downloadFile extra");
    ingest(
        &mut driver,
        &file(93, "/tmp/a.jpg", true).replacen("{", &format!(r#"{{"@extra":"{extra}","#), 1),
    );
    ingest(&mut driver, tick);
    assert_eq!(downloads(&recorder, 93), 1, "completed avatar not re-sent");
    ingest(
        &mut driver,
        &format!(r#"{{"@type":"updateFile","file":{}}}"#, file(93, "", false)),
    );
    ingest(&mut driver, tick);
    assert_eq!(
        downloads(&recorder, 93),
        2,
        "evicted avatar downloads again"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn failed_automatic_downloads_are_not_retried_on_every_ingest() {
    // An automatic download that TDLib stops (active → idle without
    // completing) or refuses must not be re-sent by the next ingest —
    // that turns every update into another `downloadFile`. Telegram X
    // treats a stopped download as paused until the user asks again
    // (`TdlibFilesManager.onFileUpdate` → `STATE_PAUSED`).
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    };
    let file = |id: i32, active: bool| {
        format!(
            r#"{{"@type":"file","id":{id},"size":24,"expected_size":24,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":{active},"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"x","unique_id":"u{id}","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}}}"#
        )
    };
    let downloads = |recorder: &RecordingSender, id: i32| {
        recorder
            .snapshot()
            .iter()
            .filter(|j| {
                j.contains("\"@type\":\"downloadFile\"") && j.contains(&format!("\"file_id\":{id}"))
            })
            .count()
    };
    for (chat_id, file_id) in [(7, 91), (8, 92)] {
        ingest(
            &mut driver,
            &format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"c","type":{{"@type":"chatTypePrivate","user_id":{chat_id}}},"unread_count":0,"photo":{{"@type":"chatPhotoInfo","small":{},"big":null,"minithumbnail":null,"has_animation":false,"is_personal":false}}}}}}"#,
                file(file_id, false)
            ),
        );
    }
    assert_eq!(downloads(&recorder, 91), 1);
    assert_eq!(downloads(&recorder, 92), 1);
    let extra_for = |recorder: &RecordingSender, id: i32| {
        recorder
            .snapshot()
            .iter()
            .filter_map(|j| serde_json::from_str::<Value>(j).ok())
            .find(|v| v["@type"] == "downloadFile" && v["file_id"] == id)
            .and_then(|v| v["@extra"].as_str().map(str::to_string))
            .expect("downloadFile extra")
    };
    // 91: TDLib accepts (`downloadFile` answers the active file at once),
    // then stops the download without completing it.
    let accepted = file(91, true).replacen(
        "{",
        &format!(r#"{{"@extra":"{}","#, extra_for(&recorder, 91)),
        1,
    );
    ingest(&mut driver, &accepted);
    ingest(
        &mut driver,
        &format!(r#"{{"@type":"updateFile","file":{}}}"#, file(91, true)),
    );
    ingest(
        &mut driver,
        &format!(r#"{{"@type":"updateFile","file":{}}}"#, file(91, false)),
    );
    // 92: TDLib refuses the request.
    let extra = extra_for(&recorder, 92);
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"error","@extra":"{extra}","code":400,"message":"FILE_ID_INVALID"}}"#
        ),
    );
    for _ in 0..3 {
        ingest(
            &mut driver,
            r#"{"@type":"updateChatTitle","chat_id":7,"title":"c"}"#,
        );
    }
    assert_eq!(downloads(&recorder, 91), 1, "stopped download not re-sent");
    assert_eq!(downloads(&recorder, 92), 1, "refused download not re-sent");
    // The user can still ask for the file explicitly.
    assert!(
        driver
            .download_user_file(FileId(91), None)
            .unwrap()
            .is_some()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_transfer_tdlib_runs_on_its_own_can_be_cancelled() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
        let owned = copy_and_parse(json, &seq, &dyn_sink).expect("parses");
        driver.ingest(owned).expect("ingests");
    };
    ingest(
        &mut driver,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
    );
    let file = |active: bool| {
        format!(
            r#"{{"@type":"updateFile","file":{{"@type":"file","id":9,"size":100,"expected_size":100,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":{active},"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":40,"downloaded_size":40}},"remote":{{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":100}}}}}}"#
        )
    };
    // Another device's download, resumed by TDLib: none of ours.
    ingest(&mut driver, &file(true));
    let downloads = |driver: &ConnectDriver<Arc<RecordingSender>>| {
        driver
            .session
            .media
            .downloads
            .is_downloading(9, &driver.session.media.files)
    };
    assert!(downloads(&driver), "the row shows the transfer");
    assert_eq!(driver.cancel_download(FileId(9)), Ok(true));
    let sent = recorder.snapshot();
    let cancel: Value = serde_json::from_str(sent.last().expect("a request")).unwrap();
    assert_eq!(cancel["@type"], "cancelDownloadFile");
    assert_eq!(cancel["file_id"], 9);
    assert!(!downloads(&driver), "the row stops at once");
    assert!(
        !driver.session.should_download(FileId(9)),
        "no automatic download undoes the cancel"
    );
    // TDLib confirms: the file is idle and nothing is left behind.
    ingest(&mut driver, &file(false));
    assert!(!downloads(&driver));
    assert_eq!(driver.cancel_download(FileId(9)), Ok(false), "nothing left");
    // Asked for again: it downloads.
    assert!(matches!(
        driver.download_user_file(FileId(9), None),
        Ok(Some(_))
    ));
    // Cancelled and asked for again before TDLib reports it idle: it
    // downloads again.
    ingest(&mut driver, &file(true));
    assert_eq!(driver.cancel_download(FileId(9)), Ok(true));
    assert!(matches!(
        driver.download_user_file(FileId(9), None),
        Ok(Some(_))
    ));
    assert!(downloads(&driver));
    let _ = std::fs::remove_dir_all(&dir);
}

// REVIEW-ONLY throwaway tests, never committed.
fn avatar_driver() -> (
    ConnectDriver<Arc<RecordingSender>>,
    Arc<RecordingSender>,
    Arc<dyn DiagnosticSink>,
    std::path::PathBuf,
) {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    (driver, recorder, dyn_sink, dir)
}

fn avatar_downloads(recorder: &RecordingSender, id: i32) -> Vec<String> {
    let needle = format!("\"file_id\":{id}");
    recorder
        .snapshot()
        .iter()
        .filter(|j| {
            j.contains("downloadFile") && !j.contains("cancelDownloadFile") && j.contains(&needle)
        })
        .cloned()
        .collect()
}

fn avatar_user(id: i64, file: i32) -> String {
    format!(
        r#"{{"@type":"updateUser","user":{{"id":{id},"first_name":"U","type":{{"@type":"userTypeRegular"}},"profile_photo":{{"@type":"profilePhoto","small":{{"@type":"file","id":{file}}},"big":null}}}}}}"#
    )
}

fn tick_on_screen(driver: &mut ConnectDriver<Arc<RecordingSender>>) {
    driver.poll_ticks(
        crate::connect::TickScope::OnScreen {
            settings_open: false,
        },
        std::time::Instant::now(),
        ONLINE,
    );
}

fn error_answer(request: &str, message: &str) -> String {
    let extra = serde_json::from_str::<Value>(request).unwrap()["@extra"]
        .as_str()
        .unwrap()
        .to_owned();
    format!(r#"{{"@type":"error","@extra":"{extra}","code":400,"message":"{message}"}}"#)
}

#[test]
fn a_claimed_avatar_survives_an_error_a_cancel_and_data_saver() {
    let (mut driver, recorder, dyn_sink, dir) = avatar_driver();
    let seq = AtomicU64::new(0);
    let feed = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    };
    feed(
        &mut driver,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
    );
    // 1. An error reply: the count drops and a frame's claim sends nothing more.
    feed(&mut driver, &avatar_user(42, 92));
    assert_eq!(driver.session.user_photo_path(42), None);
    tick_on_screen(&mut driver);
    let sent = avatar_downloads(&recorder, 92);
    assert_eq!(sent.len(), 1);
    assert!(driver.session.requests.has_download(FileId(92)));
    feed(&mut driver, &error_answer(&sent[0], "FILE_ID_INVALID"));
    assert!(!driver.session.requests.has_download(FileId(92)));
    for _ in 0..5 {
        assert_eq!(driver.session.user_photo_path(42), None);
        tick_on_screen(&mut driver);
    }
    assert_eq!(
        avatar_downloads(&recorder, 92).len(),
        1,
        "failed file re-sent"
    );
    // 2. A cancel with the answer still out, then the answer.
    feed(&mut driver, &avatar_user(43, 93));
    assert_eq!(driver.session.user_photo_path(43), None);
    tick_on_screen(&mut driver);
    let sent = avatar_downloads(&recorder, 93);
    assert_eq!(sent.len(), 1);
    assert!(driver.cancel_download(FileId(93)).unwrap());
    assert!(
        driver.session.requests.has_download(FileId(93)),
        "answer still out"
    );
    feed(&mut driver, &error_answer(&sent[0], "cancelled"));
    assert!(!driver.session.requests.has_download(FileId(93)));
    // 3. Data saver: nothing goes out; the claim is dropped, not kept.
    feed(&mut driver, &avatar_user(44, 94));
    driver.session.settings.media_prefs.data_saver = true;
    assert_eq!(driver.session.user_photo_path(44), None);
    tick_on_screen(&mut driver);
    assert!(avatar_downloads(&recorder, 94).is_empty());
    driver.session.settings.media_prefs.data_saver = false;
    tick_on_screen(&mut driver);
    assert!(
        avatar_downloads(&recorder, 94).is_empty(),
        "the claim taken under data saver is gone until a frame asks again"
    );
    assert_eq!(driver.session.user_photo_path(44), None);
    tick_on_screen(&mut driver);
    assert_eq!(avatar_downloads(&recorder, 94).len(), 1);
    // 4. Background scope never drains.
    feed(&mut driver, &avatar_user(45, 95));
    assert_eq!(driver.session.user_photo_path(45), None);
    driver.poll_ticks(
        crate::connect::TickScope::Background,
        std::time::Instant::now(),
        ONLINE,
    );
    assert!(avatar_downloads(&recorder, 95).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn only_a_claim_the_next_pass_sends_wakes_it() {
    let (mut driver, recorder, dyn_sink, dir) = avatar_driver();
    let seq = AtomicU64::new(0);
    let feed = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    };
    let wake = crate::wake::Signal::new();
    feed(
        &mut driver,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
    );
    feed(&mut driver, &avatar_user(46, 96));
    assert_eq!(driver.session.user_photo_path_waking(46, &wake), None);
    assert_eq!(wake.epoch(), 1, "the claim wakes the pass that sends it");
    assert_eq!(driver.session.user_photo_path_waking(46, &wake), None);
    assert_eq!(wake.epoch(), 1, "a repeat before that pass costs nothing");
    tick_on_screen(&mut driver);
    assert_eq!(avatar_downloads(&recorder, 96).len(), 1);
    for _ in 0..3 {
        assert_eq!(driver.session.user_photo_path_waking(46, &wake), None);
        tick_on_screen(&mut driver);
    }
    assert_eq!(wake.epoch(), 1, "a file in flight never wakes the loop");
    let sent = avatar_downloads(&recorder, 96);
    feed(&mut driver, &error_answer(&sent[0], "FILE_ID_INVALID"));
    for _ in 0..3 {
        assert_eq!(driver.session.user_photo_path_waking(46, &wake), None);
        tick_on_screen(&mut driver);
    }
    assert_eq!(avatar_downloads(&recorder, 96).len(), 1);
    assert_eq!(
        wake.epoch(),
        1,
        "a failed file is not sent, so it never wakes"
    );
    feed(&mut driver, &avatar_user(47, 97));
    driver.session.settings.media_prefs.data_saver = true;
    assert_eq!(driver.session.user_photo_path_waking(47, &wake), None);
    assert_eq!(wake.epoch(), 1, "data saver sends nothing");
    let _ = std::fs::remove_dir_all(&dir);
}
