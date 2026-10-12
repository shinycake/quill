//! Driver tests: a new caption for a media message that is still being
//! sent, Up on it, and silent or scheduled media sends.
use super::*;
use crate::composer::{
    AttachmentKind, ComposerAttachment, ComposerEdit, ComposerScheduling, ComposerSnapshot,
    SendOptions,
};

const PENDING: i64 = 90_000_000_001;
const SENT: i64 = 61;

fn photo_json(extra: &str, id: i64, caption: &str, uploading: bool) -> String {
    format!(
        r#"{{"@type":"message","@extra":"{extra}","id":{id},"chat_id":7,"is_outgoing":true,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{{"@type":"file","id":50,"size":3,"expected_size":3,"local":{{"@type":"localFile","path":"","can_be_downloaded":false,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":{uploading},"is_uploading_completed":false,"uploaded_size":0}}}},"width":100,"height":80,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"{caption}","entities":[]}},"has_spoiler":false,"is_secret":false}}}}"#
    )
}

struct Setup {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    seq: AtomicU64,
    sink: Arc<dyn DiagnosticSink>,
    extra: String,
    dir: std::path::PathBuf,
}

impl Setup {
    fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    fn sent(&self) -> Vec<Value> {
        self.recorder
            .snapshot()
            .iter()
            .map(|json| serde_json::from_str(json).unwrap())
            .collect()
    }
}

/// A photo send is on its way: `sendMessage` went out, the pending message
/// is in the history.
fn photo_in_flight(options: SendOptions) -> Setup {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    driver.select_chat(ChatId(7)).unwrap();
    let photo = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("docs/screenshots/fixtures/demo-thumb.png");
    let attachment = ComposerAttachment::pick(&photo, AttachmentKind::Photo).unwrap();
    let snap = ComposerSnapshot::capture_with_attachment(
        ChatId(7),
        driver.session.view_generation,
        "first",
        Some(attachment),
    )
    .with_send_options(options);
    let extra = driver.send_snapshot(&snap).unwrap().0.to_string();
    let mut setup = Setup {
        driver,
        recorder,
        seq,
        sink: dyn_sink,
        extra,
        dir,
    };
    let pending = photo_json(&setup.extra.clone(), PENDING, "first", true);
    setup.ingest(&pending);
    setup
}

fn succeed(setup: &mut Setup, caption: &str) {
    let message = photo_json("0", SENT, caption, false).replace(r#","@extra":"0""#, "");
    setup.ingest(&format!(
        r#"{{"@type":"updateMessageSendSucceeded","message":{message},"old_message_id":{PENDING}}}"#
    ));
}

#[test]
fn up_on_a_pending_media_message_edits_its_caption_and_sends_it_with_the_real_id() {
    let mut s = photo_in_flight(SendOptions::default());
    // The last message is still being sent: Up opens its caption.
    let edit = s
        .driver
        .session
        .last_editable_message(ChatId(7))
        .expect("the pending photo opens for editing");
    assert_eq!(edit.message_id, MessageId(PENDING));
    assert_eq!(edit.original_text, "first");
    assert!(!edit.allows_replace(), "the file cannot be swapped yet");

    let sent_before = s.sent().len();
    s.driver.edit_pending_caption(&edit, " second ").unwrap();
    // Nothing goes out while the message has only a temporary id, and the
    // row shows the new caption at once.
    assert_eq!(s.sent().len(), sent_before);
    let row = s.driver.session.histories[&7].messages[&PENDING].clone();
    assert!(row.pending);
    assert_eq!(row.content.preview(), "second");

    // The upload finishes: the caption is sent as an edit of the real id.
    succeed(&mut s, "first");
    let edit = s.sent().into_iter().last().unwrap();
    assert_eq!(edit["@type"], "editMessageCaption");
    assert_eq!(edit["chat_id"], 7);
    assert_eq!(edit["message_id"], SENT);
    assert_eq!(edit["caption"]["text"], "second");
    assert!(s.driver.session.messages.deferred_captions.is_empty());
    let _ = std::fs::remove_dir_all(&s.dir);
}

#[test]
fn an_unchanged_caption_or_a_failed_send_sends_no_edit() {
    let mut s = photo_in_flight(SendOptions::default());
    let edit = s.driver.session.last_editable_message(ChatId(7)).unwrap();
    s.driver.edit_pending_caption(&edit, "first").unwrap();
    let before = s.sent().len();
    succeed(&mut s, "first");
    assert_eq!(s.sent().len(), before, "same caption: no edit");

    let mut s = photo_in_flight(SendOptions::default());
    let edit = s.driver.session.last_editable_message(ChatId(7)).unwrap();
    s.driver.edit_pending_caption(&edit, "other").unwrap();
    let before = s.sent().len();
    let failed = photo_json("0", PENDING, "first", false).replace(r#","@extra":"0""#, "");
    s.ingest(&format!(
        r#"{{"@type":"updateMessageSendFailed","message":{failed},"old_message_id":{PENDING},"error":{{"@type":"error","code":400,"message":"nope"}}}}"#
    ));
    assert_eq!(s.sent().len(), before);
    assert!(s.driver.session.messages.deferred_captions.is_empty());
    let _ = std::fs::remove_dir_all(&s.dir);
}

#[test]
fn up_leaves_a_pending_text_alone() {
    let mut s = photo_in_flight(SendOptions::default());
    // A pending text message after the photo: Up scrolls instead.
    s.ingest(&format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{},"chat_id":7,"is_outgoing":true,"sending_state":{{"@type":"messageSendingStatePending"}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"typing","entities":[]}}}}}}}}"#,
        PENDING + 1
    ));
    let history = &mut s.driver.session.histories.get_mut(&7).unwrap().messages;
    history.get_mut(&(PENDING + 1)).unwrap().pending = true;
    assert!(s.driver.session.last_editable_message(ChatId(7)).is_none());
    assert!(
        ComposerEdit::from_pending_media(
            ChatId(7),
            MessageId(PENDING + 1),
            &s.driver.session.histories[&7].messages[&(PENDING + 1)].content
        )
        .is_none()
    );
    let _ = std::fs::remove_dir_all(&s.dir);
}

#[test]
fn silent_and_scheduled_media_sends_carry_their_options() {
    let s = photo_in_flight(SendOptions {
        disable_notification: true,
        scheduling: ComposerScheduling::SendAtDate(2_000_000_000),
        schedule_repeat_period: 86_400,
        ..SendOptions::default()
    });
    let send = s
        .sent()
        .into_iter()
        .find(|v| v["@type"] == "sendMessage")
        .expect("sendMessage");
    assert_eq!(send["input_message_content"]["@type"], "inputMessagePhoto");
    assert_eq!(send["options"]["disable_notification"], true);
    let state = &send["options"]["scheduling_state"];
    assert_eq!(state["@type"], "messageSchedulingStateSendAtDate");
    assert_eq!(state["send_date"], 2_000_000_000);
    assert_eq!(state["repeat_period"], 86_400);
    let _ = std::fs::remove_dir_all(&s.dir);

    // A plain send keeps `options: null`.
    let plain = photo_in_flight(SendOptions::default());
    let send = plain
        .sent()
        .into_iter()
        .find(|v| v["@type"] == "sendMessage")
        .unwrap();
    assert!(send["options"].is_null());
    let _ = std::fs::remove_dir_all(&plain.dir);
}

#[test]
fn up_skips_forwarded_messages_and_goes_on_to_older_ones() {
    let mut s = photo_in_flight(SendOptions::default());
    // The photo has gone; two own messages are left, the newer forwarded.
    succeed(&mut s, "first");
    s.driver
        .session
        .histories
        .get_mut(&7)
        .unwrap()
        .messages
        .clear();
    for (id, text, forwarded) in [(50, "mine", false), (55, "forwarded", true)] {
        let forward = if forwarded {
            r#","forward_info":{"@type":"messageForwardInfo","origin":{"@type":"messageOriginHiddenUser","sender_name":"x"},"date":1,"source":null}"#
        } else {
            ""
        };
        s.ingest(&format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":7,"is_outgoing":true{forward},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}}}}"#
        ));
    }
    let edit = s
        .driver
        .session
        .last_editable_message(ChatId(7))
        .expect("an editable message");
    assert_eq!(edit.message_id, MessageId(50));
    let _ = std::fs::remove_dir_all(&s.dir);
}
