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

mod downloads;
mod sends;

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
    assert_eq!(driver.session.stickers.stickers.selected_set_id, Some(77));
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
    assert_eq!(driver.session.stickers.stickers.stickers.len(), 1);
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
    assert_eq!(driver.session.stickers.gifs.animations.len(), 1);
    assert_eq!(
        driver.session.stickers.gifs.animations[0].file_id,
        FileId(33)
    );
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
        .send_voice_note(&draft, "", Some(SendReply::plain(MessageId(4))), false)
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
    assert_eq!(
        sent["input_message_content"]["self_destruct_type"],
        Value::Null,
        "a normal note is not one-time"
    );
    // Play once in a private chat asks for an immediate self-destruct.
    driver.send_voice_note(&draft, "", None, true).unwrap();
    let once = recorder
        .snapshot()
        .into_iter()
        .rev()
        .find(|json| json.contains("inputMessageVoiceNote"))
        .expect("send play-once voice");
    let once: Value = serde_json::from_str(&once).unwrap();
    assert_eq!(
        once["input_message_content"]["self_destruct_type"]["@type"],
        "messageSelfDestructTypeImmediately"
    );
    // A group refuses self-destruct, so the choice is dropped there.
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateNewChat","chat":{"id":-9,"title":"Team","type":{"@type":"chatTypeBasicGroup","basic_group_id":9},"unread_count":0}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    driver.select_chat(ChatId(-9)).unwrap();
    driver.send_voice_note(&draft, "", None, true).unwrap();
    let group = recorder
        .snapshot()
        .into_iter()
        .rev()
        .find(|json| json.contains("inputMessageVoiceNote"))
        .expect("send group voice");
    let group: Value = serde_json::from_str(&group).unwrap();
    assert_eq!(group["chat_id"], -9);
    assert_eq!(
        group["input_message_content"]["self_destruct_type"],
        Value::Null
    );
    driver.select_chat(ChatId(7)).unwrap();
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
