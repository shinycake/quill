use crate::ids::{ChatId, FileId, MessageId, RequestId};
use crate::telegram::requests::*;
use serde_json::Value;

#[test]
fn send_photo_shape_matches_1_8_67() {
    let json = send_photo(
        RequestId(11),
        ChatId(7),
        None,
        "/tmp/picked.png",
        "CANARY_CAP",
        false,
        None,
        None,
        false,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "sendMessage");
    assert_eq!(v["@extra"], "11");
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["topic_id"], Value::Null);
    assert_eq!(v["input_message_content"]["@type"], "inputMessagePhoto");
    assert_eq!(
        v["input_message_content"]["photo"]["photo"]["@type"],
        "inputFileLocal"
    );
    assert_eq!(
        v["input_message_content"]["photo"]["photo"]["path"],
        "/tmp/picked.png"
    );
    assert_eq!(
        v["input_message_content"]["photo"]["thumbnail"],
        Value::Null
    );
    assert_eq!(v["input_message_content"]["photo"]["video"], Value::Null);
    assert_eq!(v["input_message_content"]["photo"]["width"], 0);
    assert_eq!(v["input_message_content"]["photo"]["height"], 0);
    assert_eq!(v["input_message_content"]["caption"]["text"], "CANARY_CAP");
    assert_eq!(
        v["input_message_content"]["show_caption_above_media"],
        false
    );
    assert_eq!(v["input_message_content"]["has_spoiler"], false);
    assert_eq!(
        v["input_message_content"]["self_destruct_type"],
        Value::Null
    );
    assert!(!json.contains("message_thread_id"));
}

/// `inputMessageVideo` (schema 1.8.67, lines 5915/5918/6117/6128).
#[test]
fn send_photo_self_destruct_shapes() {
    for (choice, type_name) in [
        (None, None),
        (
            Some(SelfDestructSend::Timer(30)),
            Some("messageSelfDestructTypeTimer"),
        ),
        (
            Some(SelfDestructSend::Immediately),
            Some("messageSelfDestructTypeImmediately"),
        ),
    ] {
        let json = send_photo(
            RequestId(11),
            ChatId(7),
            None,
            "/tmp/picked.png",
            "cap",
            false,
            None,
            choice,
            false,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let sd = &v["input_message_content"]["self_destruct_type"];
        match type_name {
            None => assert_eq!(sd, &Value::Null),
            Some(name) => assert_eq!(sd["@type"], name),
        }
    }
    // Timer carries `self_destruct_time`; Immediately carries no fields.
    let json = send_photo(
        RequestId(11),
        ChatId(7),
        None,
        "/tmp/picked.png",
        "cap",
        false,
        None,
        Some(SelfDestructSend::Timer(30)),
        false,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        v["input_message_content"]["self_destruct_type"]["self_destruct_time"],
        30
    );
    let json = send_video(
        RequestId(17),
        ChatId(7),
        None,
        "/tmp/picked.mp4",
        &VideoSend {
            duration: 1,
            width: 320,
            height: 180,
            supports_streaming: true,
            self_destruct: Some(SelfDestructSend::Immediately),
        },
        "cap",
        false,
        None,
        false,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        v["input_message_content"]["self_destruct_type"]["@type"],
        "messageSelfDestructTypeImmediately"
    );
    assert!(
        v["input_message_content"]["self_destruct_type"]
            .get("self_destruct_time")
            .is_none()
    );
}

#[test]
fn send_video_shape_matches_1_8_67() {
    let json = send_video(
        RequestId(17),
        ChatId(7),
        None,
        "/tmp/picked.mp4",
        &VideoSend {
            duration: 1,
            width: 320,
            height: 180,
            supports_streaming: true,
            self_destruct: None,
        },
        "CANARY_VIDEO",
        false,
        Some(SendReply::plain(MessageId(9))),
        false,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "sendMessage");
    assert_eq!(v["@extra"], "17");
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["topic_id"], Value::Null);
    assert_eq!(v["input_message_content"]["@type"], "inputMessageVideo");
    let video = &v["input_message_content"]["video"];
    assert_eq!(video["@type"], "inputVideo");
    assert_eq!(video["video"]["@type"], "inputFileLocal");
    assert_eq!(video["video"]["path"], "/tmp/picked.mp4");
    assert_eq!(video["thumbnail"], Value::Null);
    assert_eq!(video["cover"], Value::Null);
    assert_eq!(video["start_timestamp"], 0);
    assert_eq!(video["added_sticker_file_ids"], serde_json::json!([]));
    assert_eq!(video["duration"], 1);
    assert_eq!(video["width"], 320);
    assert_eq!(video["height"], 180);
    assert_eq!(video["supports_streaming"], true);
    assert_eq!(
        v["input_message_content"]["caption"]["text"],
        "CANARY_VIDEO"
    );
    assert_eq!(
        v["input_message_content"]["show_caption_above_media"],
        false
    );
    assert_eq!(
        v["input_message_content"]["self_destruct_type"],
        Value::Null
    );
    assert_eq!(v["input_message_content"]["has_spoiler"], false);
    assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToMessage");
    assert_eq!(v["reply_to"]["message_id"], 9);
    assert!(!json.contains("inputMessageVideoNote"));
    assert!(!json.contains("api_hash"));
}

#[test]
fn send_video_note_shape_matches_1_8_67() {
    let json = send_video_note(
        RequestId(18),
        ChatId(7),
        None,
        "/tmp/round.mp4",
        &VideoNoteSend {
            duration: 1,
            length: 240,
            thumbnail: Some(VideoNoteThumbnailSend {
                path: "/tmp/round.jpg".into(),
                width: 240,
                height: 240,
            }),
        },
        Some(SendReply::plain(MessageId(9))),
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "sendMessage");
    assert_eq!(v["@extra"], "18");
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["input_message_content"]["@type"], "inputMessageVideoNote");
    let note = &v["input_message_content"]["video_note"];
    assert_eq!(note["@type"], "inputVideoNote");
    assert_eq!(note["video_note"]["@type"], "inputFileLocal");
    assert_eq!(note["video_note"]["path"], "/tmp/round.mp4");
    assert_eq!(note["thumbnail"]["@type"], "inputThumbnail");
    assert_eq!(note["thumbnail"]["thumbnail"]["path"], "/tmp/round.jpg");
    assert_eq!(note["thumbnail"]["width"], 240);
    assert_eq!(note["thumbnail"]["height"], 240);
    assert_eq!(note["duration"], 1);
    assert_eq!(note["length"], 240);
    assert_eq!(
        v["input_message_content"]["self_destruct_type"],
        Value::Null
    );
    assert!(v["input_message_content"].get("caption").is_none());
    assert_eq!(v["reply_to"]["message_id"], 9);
    assert!(!json.contains("api_hash"));

    let bare = send_video_note(
        RequestId(19),
        ChatId(7),
        None,
        "/tmp/round.mp4",
        &VideoNoteSend {
            duration: 0,
            length: 1,
            thumbnail: None,
        },
        None,
    );
    let bare: serde_json::Value = serde_json::from_str(&bare).unwrap();
    assert_eq!(
        bare["input_message_content"]["video_note"]["thumbnail"],
        Value::Null
    );
    assert_eq!(bare["reply_to"], Value::Null);
}

#[test]
fn send_document_shape_matches_1_8_67() {
    let json = send_document(
        RequestId(12),
        ChatId(7),
        None,
        "/tmp/picked.txt",
        "",
        None,
        false,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "sendMessage");
    assert_eq!(v["input_message_content"]["@type"], "inputMessageDocument");
    assert_eq!(
        v["input_message_content"]["document"]["document"]["@type"],
        "inputFileLocal"
    );
    assert_eq!(
        v["input_message_content"]["document"]["document"]["path"],
        "/tmp/picked.txt"
    );
    assert_eq!(
        v["input_message_content"]["document"]["disable_content_type_detection"],
        false
    );
    assert_eq!(v["input_message_content"]["caption"]["text"], "");
    assert!(!json.contains("CANARY"));
}

#[test]
fn download_file_shape_matches_1_8_67() {
    let json = download_file(RequestId(12), FileId(44), 32);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "downloadFile");
    assert_eq!(v["@extra"], "12");
    assert_eq!(v["file_id"], 44);
    assert_eq!(v["priority"], 32);
    assert_eq!(v["offset"], 0);
    assert_eq!(v["limit"], 0);
    assert_eq!(v["synchronous"], false);
    assert!(!json.contains("CANARY"));
}

#[test]
fn cancel_download_file_shape_matches_1_8_67() {
    // `cancelDownloadFile file_id:int32 only_if_pending:Bool = Ok;`
    // (schema 1.8.67, line 13991).
    let json = cancel_download_file(RequestId(13), FileId(45), false);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "cancelDownloadFile");
    assert_eq!(v["@extra"], "13");
    assert_eq!(v["file_id"], 45);
    assert_eq!(v["only_if_pending"], false);
    assert!(v.as_object().unwrap().len() == 4);
    assert!(!json.contains("CANARY"));
}

#[test]
fn add_file_to_downloads_shape_matches_1_8_67() {
    // `addFileToDownloads file_id:int32 chat_id:int53 message_id:int53
    // priority:int32 = File;` (schema 1.8.67, line 14039).
    let json = add_file_to_downloads(RequestId(21), FileId(77), ChatId(5), MessageId(9), 32);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "addFileToDownloads");
    assert_eq!(v["@extra"], "21");
    assert_eq!(v["file_id"], 77);
    assert_eq!(v["chat_id"], 5);
    assert_eq!(v["message_id"], 9);
    assert_eq!(v["priority"], 32);
    assert_eq!(v.as_object().unwrap().len(), 6);
    assert!(!json.contains("CANARY"));
}

#[test]
fn toggle_download_is_paused_shape_matches_1_8_67() {
    // `toggleDownloadIsPaused file_id:int32 is_paused:Bool = Ok;`
    // (schema 1.8.67, line 14044).
    let json = toggle_download_is_paused(RequestId(22), FileId(78), true);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "toggleDownloadIsPaused");
    assert_eq!(v["@extra"], "22");
    assert_eq!(v["file_id"], 78);
    assert_eq!(v["is_paused"], true);
    assert_eq!(v.as_object().unwrap().len(), 4);
    assert!(!json.contains("CANARY"));
}

#[test]
fn remove_file_from_downloads_shape_matches_1_8_67() {
    // `removeFileFromDownloads file_id:int32 delete_from_cache:Bool = Ok;`
    // (schema 1.8.67, line 14050).
    let json = remove_file_from_downloads(RequestId(23), FileId(79), false);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "removeFileFromDownloads");
    assert_eq!(v["@extra"], "23");
    assert_eq!(v["file_id"], 79);
    assert_eq!(v["delete_from_cache"], false);
    assert_eq!(v.as_object().unwrap().len(), 4);
    assert!(!json.contains("CANARY"));
}

#[test]
fn send_voice_note_shape_matches_1_8_67() {
    let json = send_voice_note(
        RequestId(15),
        ChatId(7),
        VoiceNoteSend {
            path: "/tmp/picked.ogg",
            duration: 3,
            waveform_b64: "BASE64WAVE",
            caption: "",
            reply_to: Some(SendReply::plain(MessageId(9))),
            topic_id: None,
            self_destruct: None,
        },
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "sendMessage");
    assert_eq!(v["@extra"], "15");
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["topic_id"], Value::Null);
    assert_eq!(v["input_message_content"]["@type"], "inputMessageVoiceNote");
    assert_eq!(
        v["input_message_content"]["voice_note"]["@type"],
        "inputVoiceNote"
    );
    assert_eq!(
        v["input_message_content"]["voice_note"]["voice_note"]["@type"],
        "inputFileLocal"
    );
    assert_eq!(
        v["input_message_content"]["voice_note"]["voice_note"]["path"],
        "/tmp/picked.ogg"
    );
    assert_eq!(v["input_message_content"]["voice_note"]["duration"], 3);
    assert_eq!(
        v["input_message_content"]["voice_note"]["waveform"],
        "BASE64WAVE"
    );
    assert_eq!(v["input_message_content"]["caption"], Value::Null);
    assert_eq!(
        v["input_message_content"]["self_destruct_type"],
        Value::Null
    );
    assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToMessage");
    assert_eq!(v["reply_to"]["message_id"], 9);
    assert!(!json.contains("inputMessageVideoNote"));
    assert!(!json.contains("CANARY"));
}

#[test]
fn play_once_voice_note_asks_for_an_immediate_self_destruct() {
    let json = send_voice_note(
        RequestId(16),
        ChatId(7),
        VoiceNoteSend {
            path: "/tmp/picked.ogg",
            duration: 3,
            waveform_b64: "",
            caption: "",
            reply_to: None,
            topic_id: None,
            self_destruct: Some(SelfDestructSend::Immediately),
        },
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        v["input_message_content"]["self_destruct_type"]["@type"],
        "messageSelfDestructTypeImmediately"
    );
    assert_eq!(v["input_message_content"]["@type"], "inputMessageVoiceNote");
}

#[test]
fn edit_message_media_wraps_each_replacement_kind() {
    use crate::composer::{EditMediaKind, EditMediaReplacement};
    let rep = |kind, spoiler| EditMediaReplacement {
        path: "/tmp/new.bin".into(),
        file_name: "new.bin".into(),
        kind,
        spoiler,
    };
    let video = VideoSend {
        duration: 3,
        width: 320,
        height: 180,
        supports_streaming: true,
        self_destruct: None,
    };
    for (kind, ty) in [
        (EditMediaKind::Photo, "inputMessagePhoto"),
        (EditMediaKind::Video, "inputMessageVideo"),
        (EditMediaKind::Document, "inputMessageDocument"),
        (EditMediaKind::Audio, "inputMessageAudio"),
    ] {
        let content = edit_media_content(
            &rep(kind, false),
            "/tmp/new.bin",
            "CANARY_CAP",
            true,
            Some(&video),
            false,
        )
        .expect("content");
        let json = edit_message_media(RequestId(4), ChatId(7), MessageId(9), content);
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "editMessageMedia");
        assert_eq!(v["@extra"], "4");
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["message_id"], 9);
        assert_eq!(v["reply_markup"], Value::Null);
        assert_eq!(v["input_message_content"]["@type"], ty);
        assert_eq!(v["input_message_content"]["caption"]["text"], "CANARY_CAP");
    }
}

#[test]
fn edit_media_caption_position_and_spoiler_only_on_photo_video() {
    use crate::composer::{EditMediaKind, EditMediaReplacement};
    let rep = |kind| EditMediaReplacement {
        path: "/tmp/x".into(),
        file_name: "x".into(),
        kind,
        spoiler: true,
    };
    let photo =
        edit_media_content(&rep(EditMediaKind::Photo), "/tmp/x", "c", true, None, false).unwrap();
    assert_eq!(photo["show_caption_above_media"], true);
    assert_eq!(photo["has_spoiler"], true);
    let doc = edit_media_content(
        &rep(EditMediaKind::Document),
        "/tmp/x",
        "c",
        true,
        None,
        false,
    )
    .unwrap();
    assert!(doc.get("show_caption_above_media").is_none());
    assert!(doc.get("has_spoiler").is_none());
    // A video needs its probe.
    assert!(
        edit_media_content(
            &rep(EditMediaKind::Video),
            "/tmp/x",
            "c",
            false,
            None,
            false
        )
        .is_none()
    );
}
