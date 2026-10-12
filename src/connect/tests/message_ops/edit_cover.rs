//! "Edit Cover": `editMessageMedia` for a video's new cover.
use super::*;

const VIDEO_MESSAGE: &str = r#"{"@type":"updateNewMessage","message":{"id":9,"chat_id":7,"is_outgoing":true,"date":1700000000,"content":{"@type":"messageVideo","video":{"@type":"video","duration":6,"width":640,"height":360,"file_name":"clip.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"video":{"@type":"file","id":52,"size":4,"expected_size":4,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"is_downloading_completed":false},"remote":{"@type":"remoteFile","id":"y","unique_id":"v"}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":2,"caption":{"@type":"formattedText","text":"clip","entities":[]},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}"#;

fn video_edit(driver: &ConnectDriver<Arc<RecordingSender>>) -> crate::composer::ComposerEdit {
    let message = driver
        .session
        .histories
        .get(&7)
        .and_then(|history| history.messages.get(&9))
        .expect("video message");
    crate::composer::ComposerEdit::from_own_content(
        ChatId(7),
        MessageId(9),
        message.is_outgoing,
        message.pending,
        &message.content,
    )
    .expect("editable video")
}

#[test]
fn driver_edit_cover_resends_existing_video_with_new_cover() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    driver
        .ingest(copy_and_parse(VIDEO_MESSAGE, &seq, &dyn_sink).expect("parse"))
        .expect("ingest");

    let cover = dir.join("cover.png");
    std::fs::write(&cover, [1u8]).unwrap();
    let cover = std::fs::canonicalize(&cover).unwrap();
    let mut edit = video_edit(&driver);
    edit.media_edit.cover = Some(cover.clone());

    // A private chat with someone else is not a cover peer.
    assert!(driver.edit_snapshot(&edit, "clip").is_err());

    // Saved Messages is (tdesktop `peer->isSelf()`).
    driver.session.my_user_id = Some(7);
    assert_eq!(
        edit.edit_media_tools(crate::composer::session_allows_cover(
            &driver.session,
            ChatId(7)
        )),
        [crate::composer::EditMediaTool::EditCover]
    );
    driver.edit_snapshot(&edit, "new caption").unwrap();
    let json = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "editMessageMedia");
    assert_eq!(v["message_id"], 9);
    let content = &v["input_message_content"];
    assert_eq!(content["@type"], "inputMessageVideo");
    assert_eq!(content["video"]["video"]["@type"], "inputFileId");
    assert_eq!(content["video"]["video"]["id"], 52);
    assert_eq!(
        content["video"]["cover"]["path"],
        cover.to_string_lossy().as_ref()
    );
    assert_eq!(content["video"]["start_timestamp"], 2);
    assert_eq!(content["video"]["duration"], 6);
    assert_eq!(content["caption"]["text"], "new caption");

    // A cover that is not an image is refused, and no cover means a plain
    // caption edit again.
    let not_image = dir.join("cover.txt");
    std::fs::write(&not_image, [1u8]).unwrap();
    edit.media_edit.cover = Some(not_image);
    assert!(driver.edit_snapshot(&edit, "clip").is_err());
    edit.media_edit.cover = None;
    driver.edit_snapshot(&edit, "clip").unwrap();
    let json = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "editMessageCaption");
    let _ = std::fs::remove_dir_all(&dir);
}
