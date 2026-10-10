//! Driver tests: sending photos and documents, and history paging.
use super::*;

#[test]
fn driver_sends_photo_and_document_from_picked_paths() {
    use crate::composer::{AttachmentKind, ComposerAttachment, ComposerSnapshot};
    use std::time::{SystemTime, UNIX_EPOCH};

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
    driver.select_chat(ChatId(7)).unwrap();

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pick_dir =
        std::env::temp_dir().join(format!("quill-send-pick-{}-{}", std::process::id(), nanos));
    std::fs::create_dir_all(&pick_dir).unwrap();
    let photo = pick_dir.join("out.png");
    let doc = pick_dir.join("notes.txt");
    std::fs::write(&photo, [1, 2, 3]).unwrap();
    std::fs::write(&doc, b"hello").unwrap();

    let photo_att = ComposerAttachment::pick(&photo, AttachmentKind::Photo).unwrap();
    let photo_path = photo_att.send_path_str().unwrap();
    let snap = ComposerSnapshot::capture_with_attachment(
        ChatId(7),
        driver.session.view_generation,
        "CANARYPHOTOCAP",
        Some(photo_att),
    );
    let extra = driver.send_snapshot(&snap).unwrap();
    let last = recorder.snapshot();
    let send_json = last.last().unwrap();
    let v: Value = serde_json::from_str(send_json).unwrap();
    assert_eq!(v["@type"], "sendMessage");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["input_message_content"]["@type"], "inputMessagePhoto");
    assert_eq!(
        v["input_message_content"]["photo"]["photo"]["path"],
        photo_path
    );
    assert_eq!(
        v["input_message_content"]["caption"]["text"],
        "CANARYPHOTOCAP"
    );

    // Pending message response upserts outgoing media.
    let pending = copy_and_parse(
            &format!(
                r#"{{"@type":"message","@extra":"{}","id":-5,"chat_id":7,"is_outgoing":true,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{{"@type":"file","id":50,"size":3,"expected_size":3,"local":{{"@type":"localFile","path":"","can_be_downloaded":false,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":true,"is_uploading_completed":false,"uploaded_size":0}}}},"width":100,"height":80,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"CANARY_PHOTO_CAP","entities":[]}},"has_spoiler":false,"is_secret":false}}}}"#,
                extra.0
            ),
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(pending).unwrap();
    let msg = driver
        .session
        .histories
        .get(&7)
        .unwrap()
        .messages
        .get(&-5)
        .unwrap();
    assert!(msg.pending);
    assert!(msg.is_outgoing);
    assert!(matches!(
        msg.content,
        crate::telegram::envelope::MessageContent::Photo(_)
    ));

    let doc_att = ComposerAttachment::pick(&doc, AttachmentKind::Document).unwrap();
    let doc_path = doc_att.send_path_str().unwrap();
    let doc_snap = ComposerSnapshot::capture_with_attachment(
        ChatId(7),
        driver.session.view_generation,
        "",
        Some(doc_att),
    );
    let doc_extra = driver.send_snapshot(&doc_snap).unwrap();
    let doc_json: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
    assert_eq!(doc_json["@extra"], doc_extra.0.to_string());
    assert_eq!(
        doc_json["input_message_content"]["@type"],
        "inputMessageDocument"
    );
    assert_eq!(
        doc_json["input_message_content"]["document"]["document"]["path"],
        doc_path
    );
    assert_eq!(doc_json["input_message_content"]["caption"]["text"], "");

    let clip = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("docs/screenshots/fixtures/demo-clip.mp4");
    let video_att = ComposerAttachment::pick(&clip, AttachmentKind::Video).unwrap();
    let video_path = video_att.send_path_str().unwrap();
    let video_snap = ComposerSnapshot::capture_with_attachment(
        ChatId(7),
        driver.session.view_generation,
        "CANARYVIDEOCAP",
        Some(video_att),
    );
    let video_extra = driver.send_snapshot(&video_snap).unwrap();
    let video_json: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
    assert_eq!(video_json["@extra"], video_extra.0.to_string());
    assert_eq!(
        video_json["input_message_content"]["@type"],
        "inputMessageVideo"
    );
    assert_eq!(
        video_json["input_message_content"]["video"]["video"]["path"],
        video_path
    );
    assert_eq!(
        video_json["input_message_content"]["video"]["thumbnail"],
        Value::Null
    );
    assert_eq!(video_json["input_message_content"]["video"]["duration"], 1);
    assert_eq!(video_json["input_message_content"]["video"]["width"], 320);
    assert_eq!(video_json["input_message_content"]["video"]["height"], 180);
    assert_eq!(
        video_json["input_message_content"]["video"]["supports_streaming"],
        true
    );
    assert_eq!(
        video_json["input_message_content"]["caption"]["text"],
        "CANARYVIDEOCAP"
    );

    let note = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("docs/screenshots/fixtures/demo-video-note.mp4");
    let note_att = ComposerAttachment::pick(&note, AttachmentKind::VideoNote).unwrap();
    let note_path = note_att.send_path_str().unwrap();
    let note_snap = ComposerSnapshot::capture_with_attachment(
        ChatId(7),
        driver.session.view_generation,
        "ignored caption",
        Some(note_att),
    );
    let note_extra = driver.send_snapshot(&note_snap).unwrap();
    let note_json: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
    assert_eq!(note_json["@extra"], note_extra.0.to_string());
    assert_eq!(
        note_json["input_message_content"]["@type"],
        "inputMessageVideoNote"
    );
    let sent_note = &note_json["input_message_content"]["video_note"];
    assert_eq!(sent_note["video_note"]["path"], note_path);
    assert_eq!(sent_note["duration"], 1);
    assert_eq!(sent_note["length"], 240);
    assert_eq!(
        note_json["input_message_content"]["self_destruct_type"],
        Value::Null
    );
    assert!(note_json["input_message_content"].get("caption").is_none());
    let thumb = &sent_note["thumbnail"];
    if !thumb.is_null() {
        assert_eq!(thumb["@type"], "inputThumbnail");
        assert_eq!(thumb["width"], 240);
        assert_eq!(thumb["height"], 240);
        assert!(
            thumb["thumbnail"]["path"]
                .as_str()
                .unwrap_or("")
                .ends_with(".jpg")
        );
    }
    let landscape = ComposerAttachment::pick(&clip, AttachmentKind::VideoNote).unwrap();
    let bad = ComposerSnapshot::capture_with_attachment(
        ChatId(7),
        driver.session.view_generation,
        "",
        Some(landscape),
    );
    assert_eq!(
        driver.send_snapshot(&bad),
        Err(ConnectSendError::InvalidRequest)
    );

    let album_photo = ComposerAttachment::pick(&photo, AttachmentKind::Photo).unwrap();
    let album_video = ComposerAttachment::pick(&clip, AttachmentKind::Video).unwrap();
    let album_snap = ComposerSnapshot::capture_album(
        ChatId(7),
        driver.session.view_generation,
        "CANARYALBUMCAP",
        vec![album_photo, album_video],
    )
    .with_reply(None);
    let album_extra = driver.send_snapshot(&album_snap).unwrap();
    let album_json: Value = serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap();
    assert_eq!(album_json["@type"], "sendMessageAlbum");
    assert_eq!(album_json["@extra"], album_extra.0.to_string());
    assert!(album_json.get("reply_markup").is_none());
    let contents = album_json["input_message_contents"].as_array().unwrap();
    assert_eq!(contents.len(), 2);
    assert_eq!(contents[0]["@type"], "inputMessagePhoto");
    assert_eq!(contents[0]["caption"]["text"], "");
    assert_eq!(contents[0]["show_caption_above_media"], false);
    assert_eq!(contents[1]["@type"], "inputMessageVideo");
    assert_eq!(contents[1]["caption"]["text"], "CANARYALBUMCAP");
    assert_eq!(contents[1]["show_caption_above_media"], false);
    let album_reply = copy_and_parse(
            &format!(
                r#"{{"@type":"messages","@extra":"{}","total_count":2,"messages":[{{"id":-8,"chat_id":7,"is_outgoing":true,"media_album_id":"9001","content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{{"@type":"file","id":51,"size":3,"expected_size":3,"local":{{"@type":"localFile","path":"","can_be_downloaded":false,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":true,"is_uploading_completed":false,"uploaded_size":0}}}},"width":100,"height":80,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":false,"is_secret":false}}}},{{"id":-7,"chat_id":7,"is_outgoing":true,"media_album_id":"9001","content":{{"@type":"messageVideo","video":{{"@type":"video","duration":1,"width":320,"height":180,"file_name":"clip.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"minithumbnail":null,"thumbnail":null,"video":{{"@type":"file","id":52,"size":4,"expected_size":4,"local":{{"@type":"localFile","path":"","can_be_downloaded":false,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"y","unique_id":"v","is_uploading_active":true,"is_uploading_completed":false,"uploaded_size":0}}}}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":"CANARY_ALBUM_CAP","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}]}}"#,
                album_extra.0
            ),
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(album_reply).unwrap();
    let history = driver.session.histories.get(&7).unwrap();
    let first = history.messages.get(&-8).unwrap();
    let second = history.messages.get(&-7).unwrap();
    assert!(first.pending && second.pending);
    assert_eq!(first.media_album_id, 9001);
    assert_eq!(second.media_album_id, 9001);
    assert_eq!(history.messages.get(&-5).unwrap().media_album_id, 0);

    // Reject paths that were not picked through ComposerAttachment.
    let forged = ComposerSnapshot::capture_with_attachment(
        ChatId(7),
        driver.session.view_generation,
        "",
        Some(crate::composer::ComposerAttachment {
            path: pick_dir.join("missing-forged.bin"),
            kind: AttachmentKind::Document,
            file_name: "missing-forged.bin".into(),
            spoiler: false,
        }),
    );
    assert_eq!(
        driver.send_snapshot(&forged),
        Err(ConnectSendError::InvalidRequest)
    );
    assert!(!sink.rendered().contains("CANARY_PHOTO"));
    let _ = std::fs::remove_dir_all(&pick_dir);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn history_paging_keeps_going_on_short_pages_and_stops_without_progress() {
    // `getChatHistory` (schema 1.8.67, line 11821): "the number of returned
    // messages is chosen by TDLib and can be smaller than the specified
    // limit" — a short page is not the end. A page that brings nothing
    // older than `from_message_id` (offset 0 starts "from exactly the
    // message from_message_id") is: re-sending the same request would
    // return the same page forever.
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: String| {
        driver
            .ingest(copy_and_parse(&json, &seq, &sink).unwrap())
            .unwrap();
    };
    let page = |extra: &str, ids: &[i64]| {
        let messages: Vec<String> = ids
            .iter()
            .map(|id| {
                format!(
                    r#"{{"id":{id},"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"m","entities":[]}}}}}}"#
                )
            })
            .collect();
        format!(
            r#"{{"@type":"messages","@extra":"{extra}","total_count":{},"messages":[{}]}}"#,
            ids.len(),
            messages.join(",")
        )
    };
    ingest(
        &mut driver,
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#.into(),
    );
    driver.select_chat(ChatId(7)).unwrap();
    let first = sent_request(&recorder, "getChatHistory");
    assert_eq!(first["from_message_id"], 0);
    // TDLib's usual first answer: just the last message.
    ingest(&mut driver, page(first["@extra"].as_str().unwrap(), &[300]));
    assert!(!driver.session.histories[&7].loaded_complete);
    driver
        .fetch_history()
        .unwrap()
        .expect("short page pages on");
    let second = sent_request(&recorder, "getChatHistory");
    assert_eq!(second["from_message_id"], 300);
    ingest(
        &mut driver,
        page(second["@extra"].as_str().unwrap(), &[300, 200, 100]),
    );
    assert!(!driver.session.histories[&7].loaded_complete);
    driver.fetch_history().unwrap().expect("progress pages on");
    let third = sent_request(&recorder, "getChatHistory");
    assert_eq!(third["from_message_id"], 100);
    // Only the boundary message again: nothing older exists.
    ingest(&mut driver, page(third["@extra"].as_str().unwrap(), &[100]));
    assert!(driver.session.histories[&7].loaded_complete);
    assert_eq!(driver.fetch_history().unwrap(), None);
    let _ = std::fs::remove_dir_all(&dir);
}
