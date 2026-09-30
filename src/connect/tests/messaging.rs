//! Connect-driver tests: stickers, GIFs, voice, photos, downloads.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::{AccountKey, ChatId, FileId, MessageId};
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use crate::telegram::requests::{AnimationSend, SendReply, StickerSend};
use crate::voice::VoiceDraft;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

#[test]
fn sticker_panel_loads_installed_set_and_send_uses_input_file_id() {
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
    driver.open_sticker_panel().unwrap();
    let sets_extra = recorder
        .snapshot()
        .into_iter()
        .rev()
        .find(|json| json.contains("getInstalledStickerSets"))
        .expect("installed sets");
    let sets_extra: Value = serde_json::from_str(&sets_extra).unwrap();
    assert_eq!(sets_extra["sticker_type"]["@type"], "stickerTypeRegular");
    let extra = sets_extra["@extra"].as_str().unwrap();
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"stickerSets","@extra":"{extra}","total_count":1,"sets":[{{"@type":"stickerSetInfo","id":"77","title":"Demo","name":"DemoStickers","thumbnail":null,"thumbnail_outline":null,"is_owned":false,"is_installed":true,"is_archived":false,"is_official":true,"sticker_type":{{"@type":"stickerTypeRegular"}},"needs_repainting":false,"is_allowed_as_chat_emoji_status":false,"is_viewed":true,"size":1,"covers":[]}}]}}"#
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert_eq!(driver.session.stickers.selected_set_id, Some(77));
    let set_req = recorder
        .snapshot()
        .into_iter()
        .rev()
        .find(|json| json.contains("getStickerSet"))
        .expect("getStickerSet");
    let set_req: Value = serde_json::from_str(&set_req).unwrap();
    assert_eq!(set_req["set_id"], "77");
    let set_extra = set_req["@extra"].as_str().unwrap();
    let file = r#"{"@type":"file","id":41,"size":8,"expected_size":8,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":8}}"#;
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"stickerSet","@extra":"{set_extra}","id":"77","title":"Demo","name":"DemoStickers","thumbnail":null,"thumbnail_outline":null,"is_owned":false,"is_installed":true,"is_archived":false,"is_official":true,"sticker_type":{{"@type":"stickerTypeRegular"}},"needs_repainting":false,"is_allowed_as_chat_emoji_status":false,"is_viewed":true,"stickers":[{{"@type":"sticker","id":"9001","set_id":"77","width":512,"height":512,"emoji":"😀","format":{{"@type":"stickerFormatWebp"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatWebp"}},"width":128,"height":128,"file":{file}}},"sticker":{file}}}],"emojis":[]}}"#
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert_eq!(driver.session.stickers.stickers.len(), 1);
    assert!(
        recorder
            .snapshot()
            .iter()
            .any(|json| json.contains("downloadFile") && json.contains("\"file_id\":41"))
    );
    driver
        .send_sticker(
            ChatId(7),
            StickerSend {
                file_id: FileId(41),
                emoji: "😀",
                width: 512,
                height: 512,
                thumb: Some((FileId(41), 128, 128)),
                reply_to: None,
                topic_id: None,
            },
        )
        .unwrap();
    let sent = recorder
        .snapshot()
        .into_iter()
        .rev()
        .find(|json| json.contains("inputMessageSticker"))
        .expect("send sticker");
    let sent: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(
        sent["input_message_content"]["sticker"]["sticker"]["@type"],
        "inputFileId"
    );
    assert_eq!(
        sent["input_message_content"]["sticker"]["sticker"]["id"],
        41
    );
    assert_eq!(sent["input_message_content"]["emoji"], "😀");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn gif_panel_loads_saved_animations_and_send_uses_input_animation() {
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
    driver.open_gif_panel().unwrap();
    let saved = recorder
        .snapshot()
        .into_iter()
        .rev()
        .find(|json| json.contains("getSavedAnimations"))
        .expect("saved animations");
    let saved: Value = serde_json::from_str(&saved).unwrap();
    let extra = saved["@extra"].as_str().unwrap();
    let thumb = r#"{"@type":"file","id":42,"size":4,"expected_size":4,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"t","unique_id":"tu","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":4}}"#;
    let file = r#"{"@type":"file","id":33,"size":9,"expected_size":9,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"a","unique_id":"au","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":9}}"#;
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"animations","@extra":"{extra}","animations":[{{"@type":"animation","duration":2,"width":240,"height":140,"file_name":"wave.mp4","mime_type":"video/mp4","has_stickers":false,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":120,"height":70,"file":{thumb}}},"animation":{file}}}]}}"#
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert_eq!(driver.session.gifs.animations.len(), 1);
    assert_eq!(driver.session.gifs.animations[0].file_id, FileId(33));
    assert!(
        recorder
            .snapshot()
            .iter()
            .any(|json| json.contains("downloadFile") && json.contains("\"file_id\":42"))
    );
    driver
        .send_animation(
            ChatId(7),
            AnimationSend {
                file_id: FileId(33),
                duration: 2,
                width: 240,
                height: 140,
                reply_to: None,
                topic_id: None,
            },
        )
        .unwrap();
    let sent = recorder
        .snapshot()
        .into_iter()
        .rev()
        .find(|json| json.contains("inputMessageAnimation"))
        .expect("send animation");
    let sent: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(
        sent["input_message_content"]["animation"]["@type"],
        "inputAnimation"
    );
    assert_eq!(
        sent["input_message_content"]["animation"]["animation"]["id"],
        33
    );
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateSavedAnimations","animation_ids":[33]}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(
        recorder
            .snapshot()
            .iter()
            .filter(|json| json.contains("getSavedAnimations"))
            .count()
            >= 2
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn voice_note_send_uses_input_file_local_and_recording_action() {
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
    driver.sync_voice_recording(true, 1_000).unwrap();
    driver.sync_voice_recording(true, 1_100).unwrap();
    let actions: Vec<String> = recorder
        .snapshot()
        .into_iter()
        .filter(|json| json.contains("sendChatAction"))
        .collect();
    assert_eq!(actions.len(), 1);
    assert!(actions[0].contains("chatActionRecordingVoiceNote"));
    let voice = dir.join("note.ogg");
    std::fs::write(&voice, b"OggS").unwrap();
    let draft = VoiceDraft {
        path: voice,
        duration_secs: 3,
        bars: vec![31, 0, 1],
    };
    driver
        .send_voice_note(&draft, "", Some(SendReply::plain(MessageId(4))))
        .unwrap();
    let sent = recorder
        .snapshot()
        .into_iter()
        .rev()
        .find(|json| json.contains("inputMessageVoiceNote"))
        .expect("send voice");
    let sent: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(
        sent["input_message_content"]["voice_note"]["voice_note"]["@type"],
        "inputFileLocal"
    );
    assert_eq!(sent["input_message_content"]["voice_note"]["duration"], 3);
    assert!(
        !sent["input_message_content"]["voice_note"]["waveform"]
            .as_str()
            .unwrap()
            .is_empty()
    );
    assert_eq!(sent["input_message_content"]["caption"], Value::Null);
    assert_eq!(sent["reply_to"]["message_id"], 4);
    assert!(
        recorder
            .snapshot()
            .iter()
            .any(|json| json.contains("chatActionCancel"))
    );
    let file = r#"{"@type":"file","id":4,"size":4,"expected_size":4,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":4}}"#;
    let history = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":8,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageVoiceNote","voice_note":{{"@type":"voiceNote","duration":12,"waveform":"","mime_type":"audio/ogg","speech_recognition_result":null,"voice":{file}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"is_listened":false}}}}}}"#
    );
    driver
        .ingest(copy_and_parse(&history, &seq, &dyn_sink).unwrap())
        .unwrap();
    driver.open_voice_content(ChatId(7), MessageId(8)).unwrap();
    assert!(
        recorder
            .snapshot()
            .iter()
            .any(|json| json.contains("openMessageContent"))
    );
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateMessageContentOpened","chat_id":7,"message_id":8}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    let row = driver
        .session
        .histories
        .get(&7)
        .and_then(|h| h.messages.get(&8))
        .expect("voice row");
    match &row.content {
        crate::telegram::envelope::MessageContent::VoiceNote(note) => {
            assert!(note.is_listened);
            assert_eq!(note.duration, 12);
        }
        other => panic!("{other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

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
    // `downloadFile` (thumb priority) from the ingest hook; a second
    // ingest does not re-request while the download is in flight.
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
    assert_eq!(avatar_json["priority"], THUMB_DOWNLOAD_PRIORITY);
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
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn select_channel_fetches_supergroup_profile_and_full_info() {
    // Parity slice: opening a channel sends `getSupergroup` (for the
    // header @username) and `getSupergroupFullInfo` (description,
    // subscriber count, linked discussion group); private chats send
    // neither.
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
                    r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    driver.select_chat(ChatId(13)).unwrap();
    let sent = recorder.snapshot();
    assert!(
        sent.iter()
            .any(|j| j.contains("getSupergroup") && j.contains("\"supergroup_id\":13")),
        "getSupergroup for the header username"
    );
    assert!(
        sent.iter()
            .any(|j| j.contains("getSupergroupFullInfo") && j.contains("\"supergroup_id\":13")),
        "getSupergroupFullInfo for the header extras"
    );
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let before = recorder.snapshot().len();
    driver.select_chat(ChatId(7)).unwrap();
    let after = recorder.snapshot();
    assert!(
        !after[before..].iter().any(|j| j.contains("getSupergroup")),
        "private chats must not fetch supergroup info"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

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
