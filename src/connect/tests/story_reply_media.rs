//! Connect-driver tests: story replies with a sticker, GIF or voice note.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::{AccountKey, ChatId, FileId};
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::telegram::requests::{AnimationSend, StickerSend};
use crate::voice::VoiceDraft;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

#[test]
fn driver_story_reply_media_targets_the_story_and_is_gated() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    let sticker = || StickerSend {
        file_id: FileId(40),
        emoji: "x",
        width: 512,
        height: 512,
        thumb: None,
        reply_to: None,
        topic_id: None,
    };
    // A story that takes no replies rejects every media reply.
    seed_story(&mut driver, &seq, &dyn_sink, 7, 5, "storyContentPhoto", "");
    assert_eq!(
        driver.send_story_reply_sticker(ChatId(7), 5, sticker()),
        Err(ConnectSendError::InvalidRequest)
    );
    seed_story(
        &mut driver,
        &seq,
        &dyn_sink,
        7,
        6,
        "storyContentPhoto",
        r#""can_be_replied":true,"#,
    );

    let extra = driver
        .send_story_reply_sticker(ChatId(7), 6, sticker())
        .unwrap();
    let json = recorder.snapshot().last().cloned().expect("sendMessage");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToStory");
    assert_eq!(v["reply_to"]["story_id"], 6);
    assert_eq!(v["input_message_content"]["@type"], "inputMessageSticker");

    driver
        .send_story_reply_animation(
            ChatId(7),
            6,
            AnimationSend {
                file_id: FileId(41),
                duration: 2,
                width: 320,
                height: 240,
                reply_to: None,
                topic_id: None,
            },
        )
        .unwrap();
    let json = recorder.snapshot().last().cloned().expect("sendMessage");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToStory");
    assert_eq!(v["input_message_content"]["@type"], "inputMessageAnimation");

    // A voice draft needs a real recorded file.
    let missing = VoiceDraft {
        path: dir.join("missing.ogg"),
        duration_secs: 2,
        bars: vec![1, 2, 3],
    };
    assert_eq!(
        driver.send_story_reply_voice(ChatId(7), 6, &missing),
        Err(ConnectSendError::InvalidRequest)
    );
    let path = dir.join("note.ogg");
    std::fs::write(&path, b"ogg").unwrap();
    let draft = VoiceDraft {
        path,
        duration_secs: 2,
        bars: vec![1, 2, 3],
    };
    driver.send_story_reply_voice(ChatId(7), 6, &draft).unwrap();
    let json = recorder.snapshot().last().cloned().expect("sendMessage");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToStory");
    assert_eq!(v["reply_to"]["story_poster_chat_id"], 7);
    assert_eq!(v["input_message_content"]["@type"], "inputMessageVoiceNote");

    // A round video reply.
    let video_path = dir.join("note.mp4");
    std::fs::write(&video_path, b"mp4").unwrap();
    let note = crate::video::VideoNoteDraft {
        path: video_path,
        duration_secs: 3,
        length: 384,
    };
    driver
        .send_story_reply_video_note(ChatId(7), 6, &note)
        .unwrap();
    let json = recorder.snapshot().last().cloned().expect("sendMessage");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToStory");
    assert_eq!(v["input_message_content"]["@type"], "inputMessageVideoNote");

    // A text reply carries entities: a custom emoji goes out as one.
    driver
        .send_story_reply(
            ChatId(7),
            6,
            &format!("hi {}", crate::composer::custom_emoji_markup("😀", 42)),
        )
        .unwrap();
    let json = recorder.snapshot().last().cloned().expect("sendMessage");
    let v: Value = serde_json::from_str(&json).unwrap();
    let text = &v["input_message_content"]["text"];
    assert_eq!(text["text"], "hi 😀");
    assert_eq!(
        text["entities"][0]["type"]["@type"],
        "textEntityTypeCustomEmoji"
    );
    assert_eq!(text["entities"][0]["type"]["custom_emoji_id"], "42");

    let _ = std::fs::remove_dir_all(&dir);
}
