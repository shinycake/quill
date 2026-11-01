//! Connect-driver tests: a snapshot whose files were read in the background
//! (`PreparedMedia`) is sent without the driver starting ffprobe or ffmpeg,
//! and goes to the chat it was made for, not the one open when it is sent.
use super::super::*;
use super::*;
use crate::composer::{
    AnimationSize, AttachmentKind, ComposerAttachment, ComposerSnapshot, PreparedFile,
    PreparedMedia, PreparedNote,
};
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::ChatId;
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use crate::video::{VideoNoteProbe, VideoProbe};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

struct Rig {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    dir: PathBuf,
}

impl Rig {
    /// Private chats 7 and 8, with chat 8 open.
    fn new() -> Self {
        let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
        for json in [
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":8,"title":"Bob","type":{"@type":"chatTypePrivate","user_id":8},"unread_count":0}}"#,
        ] {
            driver
                .ingest(copy_and_parse(json, &seq, &sink).unwrap())
                .unwrap();
        }
        driver.session.open_chat = Some(ChatId(8));
        Self {
            driver,
            recorder,
            dir,
        }
    }

    /// A file the picker accepts that no ffprobe could read.
    fn junk(&self, name: &str, kind: AttachmentKind) -> ComposerAttachment {
        let path = self.dir.join(name);
        std::fs::write(&path, b"not a media file").unwrap();
        ComposerAttachment::pick(&path, kind).unwrap()
    }

    fn last(&self) -> Value {
        serde_json::from_str(self.recorder.snapshot().last().expect("a request")).unwrap()
    }

    fn requests(&self) -> usize {
        self.recorder.snapshot().len()
    }

    fn snapshot(&self, chat: i64, att: ComposerAttachment) -> ComposerSnapshot {
        ComposerSnapshot::capture_with_attachment(
            ChatId(chat),
            self.driver.session.view_generation,
            "",
            Some(att),
        )
    }
}

fn probe(duration: i32) -> VideoProbe {
    VideoProbe {
        duration,
        width: 640,
        height: 360,
        supports_streaming: true,
    }
}

#[test]
fn a_prepared_video_is_sent_to_its_chat_while_another_is_open() {
    let mut rig = Rig::new();
    let att = rig.junk("clip.mp4", AttachmentKind::Video);
    // Nothing read the file ahead of the send: it cannot go.
    let plain = rig.snapshot(7, att.clone());
    let before = rig.requests();
    assert!(rig.driver.send_snapshot(&plain).is_err());
    assert_eq!(rig.requests(), before);
    // Read in the background, it goes without the driver touching it.
    let prepared = PreparedMedia::default().with_file(
        att.path.clone(),
        PreparedFile {
            video: Some(probe(42)),
            ..PreparedFile::default()
        },
    );
    let snap = rig.snapshot(7, att).with_prepared(prepared);
    rig.driver.send_snapshot(&snap).unwrap();
    let request = rig.last();
    assert_eq!(request["chat_id"], 7, "the chat Send was tapped in");
    assert_eq!(rig.driver.session.open_chat, Some(ChatId(8)));
    let content = &request["input_message_content"];
    assert_eq!(content["@type"], "inputMessageVideo");
    assert_eq!(content["video"]["duration"], 42);
    assert_eq!(content["video"]["width"], 640);
    let _ = std::fs::remove_dir_all(&rig.dir);
}

#[test]
fn prepared_round_videos_and_gifs_are_sent_without_probing() {
    let mut rig = Rig::new();
    let note = rig.junk("note.mp4", AttachmentKind::VideoNote);
    let prepared = PreparedMedia::default().with_file(
        note.path.clone(),
        PreparedFile {
            note: Some(PreparedNote {
                probe: VideoNoteProbe {
                    duration: 5,
                    length: 280,
                },
                thumbnail: None,
            }),
            ..PreparedFile::default()
        },
    );
    rig.driver
        .send_snapshot(&rig.snapshot(7, note).with_prepared(prepared))
        .unwrap();
    let content = rig.last()["input_message_content"].clone();
    assert_eq!(content["video_note"]["duration"], 5);
    assert_eq!(content["video_note"]["length"], 280);

    let mut gif = rig.junk("fun.mp4", AttachmentKind::Video);
    gif.extras.gif = true;
    let prepared = PreparedMedia::default().with_file(
        gif.path.clone(),
        PreparedFile {
            animation: Some(AnimationSize {
                duration: 3,
                width: 120,
                height: 90,
            }),
            ..PreparedFile::default()
        },
    );
    rig.driver
        .send_snapshot(&rig.snapshot(7, gif).with_prepared(prepared))
        .unwrap();
    let request = rig.last();
    assert_eq!(request["chat_id"], 7);
    let content = &request["input_message_content"];
    assert_eq!(content["@type"], "inputMessageAnimation");
    assert_eq!(content["animation"]["duration"], 3);
    let _ = std::fs::remove_dir_all(&rig.dir);
}

#[test]
fn a_prepared_album_goes_out_in_order_to_its_chat() {
    let mut rig = Rig::new();
    let first = rig.junk("a.mp4", AttachmentKind::Video);
    let second = rig.junk("b.mp4", AttachmentKind::Video);
    let prepared = PreparedMedia::default()
        .with_file(
            first.path.clone(),
            PreparedFile {
                video: Some(probe(1)),
                ..PreparedFile::default()
            },
        )
        .with_file(
            second.path.clone(),
            PreparedFile {
                video: Some(probe(2)),
                ..PreparedFile::default()
            },
        );
    let snap = ComposerSnapshot::capture_album(
        ChatId(7),
        rig.driver.session.view_generation,
        "",
        vec![first, second],
    )
    .with_prepared(prepared);
    rig.driver.send_snapshot(&snap).unwrap();
    let request = rig.last();
    assert_eq!(request["chat_id"], 7);
    let durations: Vec<_> = request["input_message_contents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|content| content["video"]["duration"].clone())
        .collect();
    assert_eq!(durations, [Value::from(1), Value::from(2)]);
    let _ = std::fs::remove_dir_all(&rig.dir);
}

#[test]
fn two_files_go_out_as_one_album_of_documents_with_the_caption_last() {
    let mut rig = Rig::new();
    let first = rig.junk("a.txt", AttachmentKind::Document);
    let second = rig.junk("b.txt", AttachmentKind::Document);
    let snap = ComposerSnapshot::capture_album(
        ChatId(7),
        rig.driver.session.view_generation,
        "notes",
        vec![first, second],
    );
    rig.driver.send_snapshot(&snap).unwrap();
    let request = rig.last();
    assert_eq!(request["@type"], "sendMessageAlbum");
    let contents = request["input_message_contents"].as_array().unwrap();
    let kinds: Vec<_> = contents.iter().map(|c| c["@type"].clone()).collect();
    assert_eq!(kinds, ["inputMessageDocument", "inputMessageDocument"]);
    assert_eq!(contents[0]["caption"]["text"], "");
    assert_eq!(contents[1]["caption"]["text"], "notes");
    let _ = std::fs::remove_dir_all(&rig.dir);
}
