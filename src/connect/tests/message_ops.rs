//! Connect-driver tests: send/edit/delete/forward/react, captions, instant view.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::{AccountKey, ChatId, MessageId};
use crate::platform::MemorySecretStore;
use crate::state::SupergroupFullInfoData;
use crate::state::{ComposerLinkPreview, RequestPurpose, Session, SupergroupMembersFetch};
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::ChannelMemberStatus;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

#[test]
fn driver_sends_a_reply_aimed_at_another_chat_as_external() {
    use crate::composer::{ComposerReplyTo, QuoteSelection};

    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    let reply = ComposerReplyTo::with_quote(
        ChatId(12),
        MessageId(40),
        "from elsewhere",
        QuoteSelection {
            text: "else".into(),
            position: 5,
        },
    );
    // Not aimed at chat 7: no reply goes out.
    let snap = crate::composer::ComposerSnapshot::capture(
        ChatId(7),
        driver.session.view_generation,
        "plain",
    )
    .with_reply(Some(reply.clone()));
    driver.send_snapshot(&snap).unwrap();
    let sent: Value = serde_json::from_str(&recorder.snapshot().last().cloned().unwrap()).unwrap();
    assert!(sent["reply_to"].is_null());

    // Aimed at chat 7: an external reply with the quote.
    let snap = crate::composer::ComposerSnapshot::capture(
        ChatId(7),
        driver.session.view_generation,
        "answer",
    )
    .with_reply(Some(reply.into_chat(ChatId(7))));
    driver.send_snapshot(&snap).unwrap();
    let sent: Value = serde_json::from_str(&recorder.snapshot().last().cloned().unwrap()).unwrap();
    assert_eq!(sent["@type"], "sendMessage");
    assert_eq!(sent["chat_id"], 7);
    assert_eq!(
        sent["reply_to"]["@type"],
        "inputMessageReplyToExternalMessage"
    );
    assert_eq!(sent["reply_to"]["chat_id"], 12);
    assert_eq!(sent["reply_to"]["message_id"], 40);
    assert_eq!(sent["reply_to"]["quote"]["text"]["text"], "else");
    assert_eq!(sent["reply_to"]["quote"]["position"], 5);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn driver_send_reply_shape_and_jump_to_replied() {
    use crate::composer::ComposerReplyTo;

    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    let snap = crate::composer::ComposerSnapshot::capture(
        ChatId(7),
        driver.session.view_generation,
        "CANARYREPLYtext",
    )
    .with_reply(Some(ComposerReplyTo::new(
        ChatId(7),
        MessageId(50),
        "hello already here",
    )));
    let extra = driver.send_snapshot(&snap).unwrap();
    let send_json = recorder.snapshot().last().cloned().expect("sendMessage");
    let v: Value = serde_json::from_str(&send_json).unwrap();
    assert_eq!(v["@type"], "sendMessage");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToMessage");
    assert_eq!(v["reply_to"]["message_id"], 50);
    assert_eq!(v["reply_to"]["quote"], Value::Null);
    assert_eq!(v["reply_to"]["checklist_task_id"], 0);
    assert_eq!(v["reply_to"]["poll_option_id"], "");
    assert!(send_json.contains("CANARYREPLYtext"));

    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewMessage","message":{"id":60,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARYREPLYtext","entities":[]}},"reply_to":{"@type":"messageReplyToMessage","chat_id":7,"message_id":50}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let reply = driver
        .session
        .histories
        .get(&7)
        .unwrap()
        .messages
        .get(&60)
        .unwrap();
    assert_eq!(
        driver.session.reply_quote_preview(reply).as_deref(),
        Some("hello already here")
    );
    assert_eq!(
        driver
            .jump_to_replied_message(reply.reply_to.as_ref().unwrap().message_id)
            .unwrap(),
        None
    );
    assert_eq!(
        driver.session.chat_search.jump,
        crate::state::ChatSearchJump::Ready {
            message_id: MessageId(50)
        }
    );
    assert!(
        driver
            .jump_to_replied_message(MessageId(40))
            .unwrap()
            .is_some()
    );
    assert!(!sink.rendered().contains("CANARYREPLY"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_send_snapshot_rejects_overlong_caption() {
    // MED4: `message_caption_length_max` (runtime `updateOption`)
    // gates media captions before any file work; the limit rides
    // the error so the UI can show it.
    use crate::composer::{AttachmentKind, ComposerAttachment};

    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    driver.session.message_caption_length_max = 4;

    let snap = crate::composer::ComposerSnapshot::capture_with_attachment(
        ChatId(7),
        driver.session.view_generation,
        "toolong",
        Some(ComposerAttachment {
            path: std::path::PathBuf::from("/tmp/does-not-exist.png"),
            kind: AttachmentKind::Photo,
            file_name: "does-not-exist.png".to_string(),
            spoiler: false,
        }),
    );
    let sent_before = recorder.snapshot().len();
    let err = driver.send_snapshot(&snap).unwrap_err();
    assert!(matches!(err, ConnectSendError::CaptionTooLong { limit: 4 }));
    // The gate runs before any file work or request — nothing new
    // went out.
    assert_eq!(recorder.snapshot().len(), sent_before);

    // At the limit the gate passes (the missing file then fails the
    // send — the gate is what this test pins).
    driver.session.message_caption_length_max = 7;
    let snap = crate::composer::ComposerSnapshot::capture_with_attachment(
        ChatId(7),
        driver.session.view_generation,
        "1234567",
        Some(ComposerAttachment {
            path: std::path::PathBuf::from("/tmp/does-not-exist.png"),
            kind: AttachmentKind::Photo,
            file_name: "does-not-exist.png".to_string(),
            spoiler: false,
        }),
    );
    let err = driver.send_snapshot(&snap).unwrap_err();
    assert!(!matches!(err, ConnectSendError::CaptionTooLong { .. }));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_send_album_rejects_overlong_caption() {
    // MED4: the album path gates on `message_caption_length_max`
    // before any request — an overlong caption refuses with the
    // limit and emits nothing.
    use crate::composer::{AttachmentKind, ComposerAttachment};

    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    driver.session.message_caption_length_max = 4;

    let snap = crate::composer::ComposerSnapshot::capture_album(
        ChatId(7),
        driver.session.view_generation,
        "toolong",
        vec![
            ComposerAttachment {
                path: std::path::PathBuf::from("/tmp/a.png"),
                kind: AttachmentKind::Photo,
                file_name: "a.png".to_string(),
                spoiler: false,
            },
            ComposerAttachment {
                path: std::path::PathBuf::from("/tmp/b.png"),
                kind: AttachmentKind::Photo,
                file_name: "b.png".to_string(),
                spoiler: false,
            },
        ],
    );
    let sent_before = recorder.snapshot().len();
    let err = driver.send_album_snapshot(&snap).unwrap_err();
    assert!(matches!(err, ConnectSendError::CaptionTooLong { limit: 4 }));
    assert_eq!(recorder.snapshot().len(), sent_before);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_edit_snapshot_rejects_overlong_caption() {
    // MED4: caption edits gate on `message_caption_length_max` too —
    // an overlong edit refuses with the limit and emits nothing.
    // The message must exist in history with a caption for the edit
    // path to reach the length gate.
    use crate::telegram::client::copy_and_parse;

    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    driver.session.message_caption_length_max = 4;

    // Inject an outgoing photo message with a caption into history.
    let msg_json = r#"{"@type":"updateNewMessage","message":{"id":9,"chat_id":7,"is_outgoing":true,"date":1700000000,"content":{"@type":"messagePhoto","photo":{"@type":"photo","has_stickers":false,"sizes":[]},"caption":{"@type":"formattedText","text":"hi","entities":[]},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}"#;
    let owned = copy_and_parse(msg_json, &seq, &dyn_sink).expect("parse msg");
    driver.ingest(owned).expect("ingest msg");

    let edit = crate::composer::ComposerEdit {
        chat_id: ChatId(7),
        message_id: crate::ids::MessageId(9),
        original_text: "hi".to_string(),
        kind: crate::composer::ComposerEditKind::Caption,
        scheduled: false,
        caption_above: false,
        media_edit: Default::default(),
        link_preview: Default::default(),
    };
    let sent_before = recorder.snapshot().len();
    let err = driver.edit_snapshot(&edit, "toolong").unwrap_err();
    assert!(matches!(err, ConnectSendError::CaptionTooLong { limit: 4 }));
    assert_eq!(recorder.snapshot().len(), sent_before);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_edit_snapshot_rejects_overlong_text() {
    // R8: text edits gate on `message_text_length_max` (tdesktop refuses
    // with `lng_edit_limit_reached`); the count is taken after markup.
    use crate::telegram::client::copy_and_parse;

    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    driver.session.message_text_length_max = 5;

    let msg_json = r#"{"@type":"updateNewMessage","message":{"id":9,"chat_id":7,"is_outgoing":true,"date":1700000000,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#;
    let owned = copy_and_parse(msg_json, &seq, &dyn_sink).expect("parse msg");
    driver.ingest(owned).expect("ingest msg");

    let edit = crate::composer::ComposerEdit {
        chat_id: ChatId(7),
        message_id: crate::ids::MessageId(9),
        original_text: "hi".to_string(),
        kind: crate::composer::ComposerEditKind::Text,
        scheduled: false,
        caption_above: false,
        media_edit: Default::default(),
        link_preview: Default::default(),
    };
    let sent_before = recorder.snapshot().len();
    let err = driver.edit_snapshot(&edit, "toolong").unwrap_err();
    assert!(matches!(err, ConnectSendError::TextTooLong { limit: 5 }));
    assert_eq!(recorder.snapshot().len(), sent_before);
    // Markers do not count: "**ab**" is two units.
    assert!(!matches!(
        driver.edit_snapshot(&edit, "**ab**"),
        Err(ConnectSendError::TextTooLong { .. })
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_edit_snapshot_with_replacement_sends_edit_message_media() {
    // B5: a replacement file turns the caption edit into
    // `editMessageMedia` carrying the new caption and caption position.
    use crate::composer::{EditMediaKind, EditMediaReplacement, EditableMedia, MediaEdit};
    use crate::telegram::client::copy_and_parse;

    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    let msg_json = r#"{"@type":"updateNewMessage","message":{"id":9,"chat_id":7,"is_outgoing":true,"date":1700000000,"content":{"@type":"messagePhoto","photo":{"@type":"photo","has_stickers":false,"sizes":[]},"caption":{"@type":"formattedText","text":"hi","entities":[]},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}"#;
    let owned = copy_and_parse(msg_json, &seq, &dyn_sink).expect("parse msg");
    driver.ingest(owned).expect("ingest msg");

    let picked = dir.join("replacement.png");
    std::fs::write(&picked, [1u8]).unwrap();
    let picked = std::fs::canonicalize(&picked).unwrap();
    let mut edit = crate::composer::ComposerEdit {
        chat_id: ChatId(7),
        message_id: crate::ids::MessageId(9),
        original_text: "hi".to_string(),
        kind: crate::composer::ComposerEditKind::Caption,
        scheduled: false,
        caption_above: true,
        media_edit: MediaEdit {
            media: Some(EditableMedia::Photo),
            in_album: false,
            replacement: Some(EditMediaReplacement {
                path: picked.clone(),
                file_name: "replacement.png".into(),
                kind: EditMediaKind::Photo,
                spoiler: true,
            }),
        },
        link_preview: Default::default(),
    };
    let extra = driver.edit_snapshot(&edit, "new caption").unwrap();
    let json = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "editMessageMedia");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["message_id"], 9);
    let content = &v["input_message_content"];
    assert_eq!(content["@type"], "inputMessagePhoto");
    assert_eq!(content["caption"]["text"], "new caption");
    assert_eq!(content["show_caption_above_media"], true);
    assert_eq!(content["has_spoiler"], true);
    assert_eq!(
        content["photo"]["photo"]["path"],
        picked.to_string_lossy().as_ref()
    );

    // Without a replacement the same edit stays `editMessageCaption`.
    edit.media_edit.replacement = None;
    driver.edit_snapshot(&edit, "only caption").unwrap();
    let json = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "editMessageCaption");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_instant_view_success_ingest() {
    // MED4: a `webPageInstantView` answer for a tracked
    // `GetWebPageInstantView` request populates `session.instant_view`
    // (the UI drains it into the reader). The URL rides
    // `instant_view_urls`, keyed by the request id.
    use crate::telegram::client::copy_and_parse;

    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    // Register a request like `open_instant_view` does.
    let extra = driver
        .session
        .request(RequestPurpose::GetWebPageInstantView, None);
    driver
        .session
        .instant_view_urls
        .insert(extra, "https://example.com/article".to_string());

    let json = format!(
        r#"{{"@type":"webPageInstantView","page_blocks":[],"@extra":"{id}"}}"#,
        id = extra.0,
    );
    let owned = copy_and_parse(&json, &seq, &dyn_sink).expect("parse IV");
    driver.ingest(owned).expect("ingest IV");
    let iv = driver.session.instant_view.as_ref().expect("IV stored");
    assert_eq!(iv.url, "https://example.com/article");
    // A success is never stashed as a browser fallback.
    assert!(driver.session.instant_view_fallback_url.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_instant_view_error_falls_back() {
    // MED4: a TDLib error for `getWebPageInstantView` (e.g. 404 — no
    // Instant View for the page) stashes the URL for browser
    // fallback, never a fake reader.
    use crate::telegram::client::copy_and_parse;

    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    let extra = driver
        .session
        .request(RequestPurpose::GetWebPageInstantView, None);
    driver
        .session
        .instant_view_urls
        .insert(extra, "https://example.com/noiv".to_string());

    let json = format!(
        r#"{{"@type":"error","code":404,"message":"Not Found","@extra":"{id}"}}"#,
        id = extra.0,
    );
    let owned = copy_and_parse(&json, &seq, &dyn_sink).expect("parse error");
    driver.ingest(owned).expect("ingest error");
    assert!(driver.session.instant_view.is_none());
    assert_eq!(
        driver.session.instant_view_fallback_url.as_deref(),
        Some("https://example.com/noiv")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_link_preview_prefetch_ingest() {
    // MED4b: a `linkPreview` answer for a tracked `GetLinkPreview`
    // request populates `session.composer_preview` (the chip reads
    // it); a 404 becomes "no link info" (`Some(None)`), never a
    // card. The URL rides `composer_preview_urls`, keyed by id.
    use crate::telegram::client::copy_and_parse;

    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    // Register a request like `request_composer_link_preview` does.
    let extra = driver.session.request(RequestPurpose::GetLinkPreview, None);
    driver
        .session
        .composer_preview_urls
        .insert(extra, "https://example.com/story".to_string());
    driver.session.composer_preview = Some(ComposerLinkPreview {
        url: "https://example.com/story".to_string(),
        preview: None,
    });

    let json = format!(
        r#"{{"@type":"linkPreview","url":"https://example.com/story","display_url":"example.com","site_name":"Example","title":"A short story","description":{{"@type":"formattedText","text":"Preview body","entities":[]}},"author":"","type":{{"@type":"linkPreviewTypeArticle"}},"has_large_media":true,"show_large_media":false,"show_media_above_description":false,"skip_confirmation":true,"show_above_text":false,"instant_view_version":0,"@extra":"{id}"}}"#,
        id = extra.0,
    );
    let owned = copy_and_parse(&json, &seq, &dyn_sink).expect("parse preview");
    driver.ingest(owned).expect("ingest preview");
    let stored = driver
        .session
        .composer_preview
        .as_ref()
        .expect("preview stored");
    assert_eq!(stored.url, "https://example.com/story");
    let preview = stored
        .preview
        .as_ref()
        .expect("loaded")
        .as_ref()
        .expect("card");
    assert_eq!(preview.title, "A short story");
    assert_eq!(preview.description, "Preview body");
    assert!(preview.has_large_media);
    assert!(!preview.show_large_media);

    // A superseded URL's late answer must not clobber the chip.
    let stale = driver.session.request(RequestPurpose::GetLinkPreview, None);
    driver
        .session
        .composer_preview_urls
        .insert(stale, "https://example.com/old".to_string());
    let json = format!(
        r#"{{"@type":"linkPreview","url":"https://example.com/old","display_url":"example.com","site_name":"","title":"Old","description":{{"@type":"formattedText","text":"","entities":[]}},"author":"","type":{{"@type":"linkPreviewTypeArticle"}},"has_large_media":false,"show_large_media":false,"show_media_above_description":false,"skip_confirmation":false,"show_above_text":false,"instant_view_version":0,"@extra":"{id}"}}"#,
        id = stale.0,
    );
    let owned = copy_and_parse(&json, &seq, &dyn_sink).expect("parse stale");
    driver.ingest(owned).expect("ingest stale");
    let stored = driver
        .session
        .composer_preview
        .as_ref()
        .expect("preview kept");
    assert_eq!(stored.url, "https://example.com/story");

    // 404 for the current URL → "no link info", never a card.
    let miss = driver.session.request(RequestPurpose::GetLinkPreview, None);
    driver
        .session
        .composer_preview_urls
        .insert(miss, "https://example.com/story".to_string());
    driver.session.composer_preview = Some(ComposerLinkPreview {
        url: "https://example.com/story".to_string(),
        preview: None,
    });
    let json = format!(
        r#"{{"@type":"error","code":404,"message":"Not Found","@extra":"{id}"}}"#,
        id = miss.0,
    );
    let owned = copy_and_parse(&json, &seq, &dyn_sink).expect("parse 404");
    driver.ingest(owned).expect("ingest 404");
    let stored = driver
        .session
        .composer_preview
        .as_ref()
        .expect("state kept");
    assert_eq!(stored.url, "https://example.com/story");
    assert!(stored.preview.as_ref().expect("resolved").is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_send_snapshot_carries_quote() {
    // Slice G1: a composer reply with a validated partial quote
    // reaches `sendMessage` as `inputTextQuote` (schema 1.8.67,
    // line 3056).
    use crate::composer::{ComposerReplyTo, QuoteSelection};

    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    let snap = crate::composer::ComposerSnapshot::capture(
        ChatId(7),
        driver.session.view_generation,
        "CANARYQUOTEtext",
    )
    .with_reply(Some(ComposerReplyTo::with_quote(
        ChatId(7),
        MessageId(50),
        "hello already here",
        QuoteSelection {
            text: "already".to_string(),
            position: 6,
        },
    )));
    let extra = driver.send_snapshot(&snap).unwrap();
    let send_json = recorder.snapshot().last().cloned().expect("sendMessage");
    let v: Value = serde_json::from_str(&send_json).unwrap();
    assert_eq!(v["@type"], "sendMessage");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToMessage");
    assert_eq!(v["reply_to"]["message_id"], 50);
    assert_eq!(v["reply_to"]["quote"]["@type"], "inputTextQuote");
    assert_eq!(v["reply_to"]["quote"]["text"]["text"], "already");
    assert_eq!(v["reply_to"]["quote"]["position"], 6);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_fetch_basic_group_members_shape() {
    // Slice G1: `getBasicGroupFullInfo` (schema 1.8.67, line 11507)
    // for a basic group chat; the answer populates
    // `basic_group_members`.
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
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":9,"title":"Group","type":{"@type":"chatTypeBasicGroup","basic_group_id":3},"permissions":{"@type":"chatPermissions","can_send_basic_messages":true},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let extra = driver
        .fetch_basic_group_members(ChatId(9))
        .unwrap()
        .expect("request sent");
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "getBasicGroupFullInfo");
    assert_eq!(v["basic_group_id"], 3);
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"basicGroupFullInfo","@extra":"{}","members":[{{"@type":"chatMember","member_id":{{"@type":"messageSenderUser","user_id":7}},"status":{{"@type":"chatMemberStatusMember"}}}}]}}"#,
                        extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let fetch = driver.session.basic_group_members.get(&9).unwrap();
    match fetch {
        SupergroupMembersFetch::Loaded { members, .. } => assert_eq!(members.len(), 1),
        other => panic!("unexpected {other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_set_chat_member_tag_gates_and_shape() {
    // Slice G1: `setChatMemberTag` (schema 1.8.67, line 13598) —
    // owner of a group may retitle; channels are rejected; tags
    // over 16 characters are rejected client-side.
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
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":9,"title":"Group","type":{"@type":"chatTypeBasicGroup","basic_group_id":3},"permissions":{"@type":"chatPermissions","can_send_basic_messages":true},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    // Not the owner → no request.
    assert!(
        driver
            .set_chat_member_tag(ChatId(9), 42, "boss")
            .unwrap()
            .is_none()
    );
    // Owner → sends `setChatMemberTag`.
    driver.session.my_user_id = Some(7);
    driver.session.chats.get_mut(&9).unwrap().my_member_status = Some(ChannelMemberStatus::Creator);
    let extra = driver
        .set_chat_member_tag(ChatId(9), 42, "boss")
        .unwrap()
        .expect("request sent");
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "setChatMemberTag");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["chat_id"], 9);
    assert_eq!(v["user_id"], 42);
    assert_eq!(v["tag"], "boss");
    // Over-long tag rejected without sending.
    assert!(
        driver
            .set_chat_member_tag(ChatId(9), 42, "this title is way too long")
            .is_err()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_group_info_edit_gates_and_shape() {
    // Slice: `setChatTitle` / `setChatDescription` / `setChatPhoto`
    // (schema 1.8.67, lines 13430/13533/13435) — basic groups,
    // supergroups and channels. Basic groups are democratic: any
    // member may edit (telegram.org/blog/supergroups), no right
    // needed. Supergroups and channels are gated on `can_change_info`
    // — creator, an admin with the right, or a plain member with
    // the default `permissions.can_change_info`. Client length
    // validation refuses invalid input before sending; a TDLib
    // `ok` is handled by the generic pending-request path — no
    // optimistic state.
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
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":9,"title":"Group","type":{"@type":"chatTypeBasicGroup","basic_group_id":3},"permissions":{"@type":"chatPermissions","can_send_basic_messages":true},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":10,"title":"Supergroup","type":{"@type":"chatTypeSupergroup","supergroup_id":10,"is_channel":false},"permissions":{"@type":"chatPermissions","can_send_basic_messages":true},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    // Unknown chat id → refused without sending.
    let sent_before = recorder.snapshot().len();
    assert!(
        driver
            .set_group_title(ChatId(404), "New")
            .unwrap()
            .is_none()
    );
    // Private chat (wrong kind) → refused without sending.
    assert!(
        driver
            .set_group_description(ChatId(7), "about")
            .unwrap()
            .is_none()
    );
    assert_eq!(recorder.snapshot().len(), sent_before);
    // Plain member of a basic group → sends: basic groups are
    // democratic — no right needed.
    let extra = driver
        .set_group_photo(ChatId(9), None)
        .unwrap()
        .expect("request sent");
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "setChatPhoto");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert!(v["photo"].is_null());
    driver
        .session
        .requests
        .take_purpose(RequestPurpose::SetChatPhoto);
    // Supergroup plain member without `permissions.can_change_info`
    // → refused.
    let sent_before = recorder.snapshot().len();
    assert!(driver.set_group_photo(ChatId(10), None).unwrap().is_none());
    assert_eq!(recorder.snapshot().len(), sent_before);
    // Supergroup plain member WITH the default
    // `permissions.can_change_info` → sends.
    driver
        .session
        .chats
        .get_mut(&10)
        .unwrap()
        .permissions
        .as_mut()
        .unwrap()
        .can_change_info = true;
    let extra = driver
        .set_group_photo(ChatId(10), None)
        .unwrap()
        .expect("request sent");
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "setChatPhoto");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert!(v["photo"].is_null());
    driver
        .session
        .requests
        .take_purpose(RequestPurpose::SetChatPhoto);
    // Creator → sends `setChatTitle`.
    driver.session.my_user_id = Some(7);
    driver.session.chats.get_mut(&9).unwrap().my_member_status = Some(ChannelMemberStatus::Creator);
    let extra = driver
        .set_group_title(ChatId(9), "New name")
        .unwrap()
        .expect("request sent");
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "setChatTitle");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["chat_id"], 9);
    assert_eq!(v["title"], "New name");
    // In-flight dedup: second title while one is pending → no-op.
    assert!(
        driver
            .set_group_title(ChatId(9), "Another")
            .unwrap()
            .is_none()
    );
    // Length validation before send: empty and 129 chars refused,
    // exactly 128 accepted by the builder (dedup keeps it unsent).
    assert!(driver.set_group_title(ChatId(9), "").is_err());
    assert!(driver.set_group_title(ChatId(9), &"x".repeat(129)).is_err());
    // 255-char description sends; 256 is refused.
    let extra = driver
        .set_group_description(ChatId(9), &"y".repeat(255))
        .unwrap()
        .expect("request sent");
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "setChatDescription");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["description"], "y".repeat(255));
    assert!(
        driver
            .set_group_description(ChatId(9), &"y".repeat(256))
            .is_err()
    );
    // Photo set → `inputChatPhotoStatic` / `inputFileLocal`.
    let extra = driver
        .set_group_photo(ChatId(9), Some("/tmp/group.jpg"))
        .unwrap()
        .expect("request sent");
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "setChatPhoto");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["photo"]["@type"], "inputChatPhotoStatic");
    assert_eq!(v["photo"]["photo"]["@type"], "inputFileLocal");
    assert_eq!(v["photo"]["photo"]["path"], "/tmp/group.jpg");
    // Photo delete → null top-level `photo`.
    driver
        .session
        .requests
        .take_purpose(RequestPurpose::SetChatPhoto);
    let extra = driver
        .set_group_photo(ChatId(9), None)
        .unwrap()
        .expect("request sent");
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "setChatPhoto");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert!(v["photo"].is_null());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_group_sticker_set_gates_and_shape() {
    // Slice S11: `setSupergroupStickerSet` /
    // `setSupergroupCustomEmojiStickerSet` (schema 1.8.67, lines
    // 15154/15159). Gated on `supergroupFullInfo.can_set_sticker_set`
    // (fail closed while unfetched); unknown chats and non-supergroup
    // chats refused without sending; negative ids refused
    // client-side; 0 removes per the schema; in-flight dedup per chat.
    // Not optimistic — the server confirms via
    // `updateSupergroupFullInfo`.
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
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":10,"title":"Supergroup","type":{"@type":"chatTypeSupergroup","supergroup_id":10,"is_channel":false},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    // Unknown chat → refused without sending.
    let sent_before = recorder.snapshot().len();
    assert!(
        driver
            .set_supergroup_sticker_set(ChatId(404), 5)
            .unwrap()
            .is_none()
    );
    // Private chat (wrong kind) → refused.
    assert!(
        driver
            .set_supergroup_sticker_set(ChatId(7), 5)
            .unwrap()
            .is_none()
    );
    assert_eq!(recorder.snapshot().len(), sent_before);
    // Full info unfetched → capability gate fails closed.
    assert!(driver.load_group_sticker_choices(ChatId(10)).is_err());
    assert!(
        driver
            .set_supergroup_sticker_set(ChatId(10), 5)
            .unwrap()
            .is_none()
    );
    assert_eq!(recorder.snapshot().len(), sent_before);
    // Negative id → invalid request.
    assert!(driver.set_supergroup_sticker_set(ChatId(10), -1).is_err());
    // Seed the capability, then sends.
    driver.session.supergroup_full_infos.insert(
        10,
        SupergroupFullInfoData {
            can_set_sticker_set: true,
            ..Default::default()
        },
    );
    driver.load_group_sticker_choices(ChatId(10)).unwrap();
    let requests: Vec<Value> = recorder
        .snapshot()
        .iter()
        .map(|s| serde_json::from_str(s).unwrap())
        .filter(|v: &Value| v["@type"] == "getInstalledStickerSets")
        .collect();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0]["sticker_type"]["@type"], "stickerTypeRegular");
    assert_eq!(
        requests[1]["sticker_type"]["@type"],
        "stickerTypeCustomEmoji"
    );
    let sent = recorder.snapshot().len();
    driver.load_group_sticker_choices(ChatId(10)).unwrap();
    assert_eq!(recorder.snapshot().len(), sent);
    assert!(!driver.session.stickers.open && !driver.session.emoji.open);
    let extra = driver
        .set_supergroup_sticker_set(ChatId(10), 1234567890123)
        .unwrap()
        .expect("request sent");
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "setSupergroupStickerSet");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["supergroup_id"], 10);
    assert_eq!(v["sticker_set_id"], "1234567890123");
    // In-flight dedup: second call while one is pending → no-op.
    assert!(
        driver
            .set_supergroup_sticker_set(ChatId(10), 6)
            .unwrap()
            .is_none()
    );
    driver
        .session
        .requests
        .take_purpose(RequestPurpose::SetSupergroupStickerSet);
    // 0 removes the group sticker set per the schema.
    let extra = driver
        .set_supergroup_sticker_set(ChatId(10), 0)
        .unwrap()
        .expect("request sent");
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["sticker_set_id"], "0");
    assert_eq!(v["@extra"], extra.0.to_string());
    driver
        .session
        .requests
        .take_purpose(RequestPurpose::SetSupergroupStickerSet);
    // Custom-emoji variant: same gating, shape, and remove encoding.
    assert!(
        driver
            .set_supergroup_custom_emoji_sticker_set(ChatId(7), 5)
            .unwrap()
            .is_none()
    );
    let extra = driver
        .set_supergroup_custom_emoji_sticker_set(ChatId(10), 9876543210987)
        .unwrap()
        .expect("request sent");
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "setSupergroupCustomEmojiStickerSet");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["supergroup_id"], 10);
    assert_eq!(v["custom_emoji_sticker_set_id"], "9876543210987");
    assert_eq!(
        driver
            .session
            .supergroup_full_info(10)
            .unwrap()
            .custom_emoji_sticker_set_id,
        0
    );
    driver.ingest(copy_and_parse(&serde_json::json!({"@type":"error","@extra":extra.as_extra(),"code":403,"message":"private body"}).to_string(), &seq, &dyn_sink).unwrap()).unwrap();
    assert!(
        driver
            .session
            .chat_action_error
            .as_deref()
            .is_some_and(|e| e.contains("Could not change") && !e.contains("private body"))
    );
    assert_eq!(
        driver
            .session
            .supergroup_full_info(10)
            .unwrap()
            .custom_emoji_sticker_set_id,
        0
    );
    assert!(
        driver
            .set_supergroup_custom_emoji_sticker_set(ChatId(10), 0)
            .unwrap()
            .is_some()
    );
    assert!(
        driver
            .set_supergroup_custom_emoji_sticker_set(ChatId(10), -1)
            .is_err()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_community_create_rename_refetch_chain() {
    // Slice (communities backend core): `createCommunity` /
    // `getCommunityFullInfo` (TDLib 1.8.68) / `setCommunityName`. Empty
    // names are refused client-side; unknown chats are refused without
    // sending; in-flight dedupe is per (purpose, chat) / (purpose,
    // community). The `communityId` answer chains into
    // `getCommunityFullInfo`,
    // and a confirmed `setCommunityName` refetches the dropped
    // full-info pack (the welcome-message-mutation pattern).
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
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":9,"title":"Group","type":{"@type":"chatTypeBasicGroup","basic_group_id":3},"permissions":{"@type":"chatPermissions","can_send_basic_messages":true},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    // Unknown chat → refused without sending.
    let sent_before = recorder.snapshot().len();
    assert!(
        driver
            .create_community(ChatId(404), "Rustaceans", false)
            .unwrap()
            .is_none()
    );
    assert_eq!(recorder.snapshot().len(), sent_before);
    // Empty / whitespace-only name → InvalidRequest.
    assert!(driver.create_community(ChatId(9), "", false).is_err());
    assert!(driver.create_community(ChatId(9), "   ", false).is_err());
    // Sends `createCommunity` with the right shape.
    let extra = driver
        .create_community(ChatId(9), "Rustaceans", true)
        .unwrap()
        .expect("request sent");
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "createCommunity");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["name"], "Rustaceans");
    assert_eq!(v["chat_id"], 9);
    assert!(v["is_chat_hidden"].as_bool() == Some(true));
    // In-flight dedup: second create while one is pending → no-op.
    assert!(
        driver
            .create_community(ChatId(9), "Other", false)
            .unwrap()
            .is_none()
    );
    // `updateCommunity` (guaranteed before the `communityId` answer)
    // creates the community; the `communityId` then chains into
    // `getCommunityFullInfo`.
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateCommunity","community":{"@type":"community","id":42,"have_access":true,"name":"Rustaceans","date":1759000000}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert!(driver.session.communities.contains_key(&42));
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"communityId","id":42,"@extra":"{}"}}"#,
                    extra.0
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "getCommunityFullInfo");
    assert_eq!(v["community_id"], 42);
    // In flight → a second fetch is a no-op.
    let sent_before = recorder.snapshot().len();
    assert!(driver.get_community_full_info(42).unwrap().is_none());
    assert_eq!(recorder.snapshot().len(), sent_before);
    // TDLib 1.8.68 answers the pack itself (no community id); the
    // pending request routes it to community 42.
    let load_extra = v["@extra"].as_str().expect("@extra").to_string();
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"communityFullInfo","@extra":"{load_extra}","photo":null,"chats":[],"administrator_count":1,"banned_count":0,"add_chat_request_count":0}}"#
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        driver
            .session
            .community_full_infos
            .get(&42)
            .map(|info| info.administrator_count),
        Some(1)
    );
    let sent_before = recorder.snapshot().len();
    assert!(driver.get_community_full_info(42).unwrap().is_none());
    assert_eq!(recorder.snapshot().len(), sent_before);
    // `setCommunityName`: empty refused, shape right, deduped in
    // flight.
    assert!(driver.set_community_name(42, "").is_err());
    let extra = driver
        .set_community_name(42, "Rustaceans+")
        .unwrap()
        .expect("request sent");
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "setCommunityName");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["community_id"], 42);
    assert_eq!(v["name"], "Rustaceans+");
    assert!(driver.set_community_name(42, "Again").unwrap().is_none());
    // `ok` drops the pack and the ingest refetches it.
    driver
        .ingest(
            copy_and_parse(
                &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(!driver.session.community_full_infos.contains_key(&42));
    let sent = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(v["@type"], "getCommunityFullInfo");
    assert_eq!(v["community_id"], 42);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_community_management_rights_and_delete() {
    // TDLib 1.8.68: `setCommunityPermissions` (can_ban_members),
    // `setCommunityPhoto` (can_change_info) and `deleteCommunity`
    // (owner). Missing rights are refused without sending; a confirmed
    // delete drops the community and its pack; errors surface in
    // `community_error`.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    };
    // 42: owned. 43: plain member. 44: admin with can_ban_members only.
    ingest(
        &mut driver,
        r#"{"@type":"updateCommunity","community":{"@type":"community","id":42,"have_access":true,"name":"Mine","date":1,"status":{"@type":"communityMemberStatusCreator"},"permissions":{"@type":"communityPermissions","can_edit_chat_list":false}}}"#,
    );
    ingest(
        &mut driver,
        r#"{"@type":"updateCommunity","community":{"@type":"community","id":43,"have_access":true,"name":"Theirs","date":1,"status":{"@type":"communityMemberStatusMember"},"permissions":{"@type":"communityPermissions","can_edit_chat_list":false}}}"#,
    );
    ingest(
        &mut driver,
        r#"{"@type":"updateCommunity","community":{"@type":"community","id":44,"have_access":true,"name":"Moderated","date":1,"status":{"@type":"communityMemberStatusAdministrator","can_be_edited":false,"rights":{"@type":"communityAdministratorRights","can_manage_community":true,"can_change_info":false,"can_edit_chat_list":false,"can_promote_members":false,"can_ban_members":true}},"permissions":{"@type":"communityPermissions","can_edit_chat_list":false}}}"#,
    );
    let sent_before = recorder.snapshot().len();
    assert!(
        driver
            .set_community_permissions(43, true)
            .unwrap()
            .is_none()
    );
    assert!(driver.set_community_photo(43, None).unwrap().is_none());
    assert!(driver.set_community_photo(44, None).unwrap().is_none());
    assert!(driver.delete_community(43).unwrap().is_none());
    assert!(driver.delete_community(44).unwrap().is_none());
    assert!(driver.delete_community(404).unwrap().is_none());
    assert_eq!(recorder.snapshot().len(), sent_before);

    // Admin with can_ban_members may change member permissions.
    let extra = driver
        .set_community_permissions(44, true)
        .unwrap()
        .expect("sent");
    let v: Value = serde_json::from_str(&recorder.snapshot().last().cloned().unwrap()).unwrap();
    assert_eq!(v["@type"], "setCommunityPermissions");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["permissions"]["can_edit_chat_list"], true);
    assert!(
        driver
            .set_community_permissions(44, false)
            .unwrap()
            .is_none()
    );
    // A refusal surfaces in the status line.
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"Have not enough rights"}}"#,
            extra.0
        ),
    );
    let err = driver
        .session
        .community_error
        .take()
        .expect("error surfaced");
    assert!(
        err.starts_with("Could not change the community permissions"),
        "{err}"
    );

    // Owner: photo (set + delete) and delete.
    driver
        .set_community_photo(42, Some("/tmp/photo.jpg"))
        .unwrap()
        .expect("sent");
    let v: Value = serde_json::from_str(&recorder.snapshot().last().cloned().unwrap()).unwrap();
    assert_eq!(v["@type"], "setCommunityPhoto");
    assert_eq!(v["photo"]["photo"]["path"], "/tmp/photo.jpg");
    let extra = driver.delete_community(42).unwrap().expect("sent");
    let v: Value = serde_json::from_str(&recorder.snapshot().last().cloned().unwrap()).unwrap();
    assert_eq!(v["@type"], "deleteCommunity");
    assert_eq!(v["community_id"], 42);
    driver.session.community_full_infos.insert(
        42,
        crate::telegram::envelope::ParsedCommunityFullInfo {
            chats: Vec::new(),
            administrator_count: 1,
            banned_count: 0,
            add_chat_request_count: 0,
        },
    );
    ingest(
        &mut driver,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(!driver.session.communities.contains_key(&42));
    assert!(!driver.session.community_full_infos.contains_key(&42));
    assert!(driver.session.communities.contains_key(&43));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_edit_text_shape_and_incoming_rejected() {
    use crate::composer::{ComposerEdit, ComposerEditKind};

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
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewMessage","message":{"id":60,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"own outgoing","entities":[]}}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

    let incoming = ComposerEdit {
        chat_id: ChatId(7),
        message_id: MessageId(50),
        original_text: "hello already here".into(),
        kind: ComposerEditKind::Text,
        scheduled: false,
        caption_above: false,
        media_edit: Default::default(),
        link_preview: Default::default(),
    };
    assert_eq!(
        driver.edit_snapshot(&incoming, "nope"),
        Err(ConnectSendError::InvalidRequest)
    );

    let edit = ComposerEdit {
        chat_id: ChatId(7),
        message_id: MessageId(60),
        original_text: "own outgoing".into(),
        kind: ComposerEditKind::Text,
        scheduled: false,
        caption_above: false,
        media_edit: Default::default(),
        link_preview: Default::default(),
    };
    assert_eq!(
        driver.edit_snapshot(&edit, "   "),
        Err(ConnectSendError::InvalidRequest)
    );
    let extra = driver.edit_snapshot(&edit, "CANARYEDITtext").unwrap();
    let json = recorder
        .snapshot()
        .last()
        .cloned()
        .expect("editMessageText");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "editMessageText");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["message_id"], 60);
    assert_eq!(v["reply_markup"], Value::Null);
    assert_eq!(v["input_message_content"]["@type"], "inputMessageText");
    assert_eq!(v["input_message_content"]["text"]["text"], "CANARYEDITtext");
    assert_eq!(v["input_message_content"]["clear_draft"], false);

    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateMessageContent","chat_id":7,"message_id":60,"new_content":{"@type":"messageText","text":{"@type":"formattedText","text":"CANARYEDITtext","entities":[]}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert_eq!(
        driver
            .session
            .histories
            .get(&7)
            .unwrap()
            .messages
            .get(&60)
            .unwrap()
            .content
            .preview(),
        "CANARYEDITtext"
    );
    assert!(!sink.rendered().contains("CANARYEDIT"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_edit_scheduled_message_uses_scheduled_list() {
    use crate::composer::{ComposerEdit, ComposerEditKind};
    use crate::ids::{ChatId, MessageId};
    use crate::telegram::envelope::{
        MessageContent, MessageSchedulingState, ParsedMessage, TextContent,
    };

    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    // A scheduled send lives in `session.scheduled_messages`, not history.
    driver.session.scheduled_messages.push(ParsedMessage {
        sender: None,
        id: MessageId(70),
        chat_id: ChatId(7),
        date: 0,
        is_outgoing: true,
        is_pinned: false,
        topic_id: None,
        thread_id: None,
        ephemeral: None,
        media_album_id: 0,
        author_signature: None,
        scheduling_state: Some(MessageSchedulingState::SendAtDate { send_date: 999 }),
        can_retry: false,
        send_state: Default::default(),
        content: MessageContent::Text(TextContent::plain("scheduled draft")),
        files: Vec::new(),
        reply_to: None,
        forward_info: None,
        extras: Default::default(),
        interaction_info: None,
        reply_markup: None,
        self_destruct: None,
        auto_delete: None,
    });

    // A non-scheduled edit for the same id finds nothing in history.
    let plain = ComposerEdit {
        chat_id: ChatId(7),
        message_id: MessageId(70),
        original_text: "scheduled draft".into(),
        kind: ComposerEditKind::Text,
        scheduled: false,
        caption_above: false,
        media_edit: Default::default(),
        link_preview: Default::default(),
    };
    assert_eq!(
        driver.edit_snapshot(&plain, "nope"),
        Err(ConnectSendError::InvalidRequest)
    );

    // The scheduled edit validates against the scheduled list and sends
    // the same `editMessageText` request.
    let scheduled = ComposerEdit {
        scheduled: true,
        ..plain
    };
    let extra = driver.edit_snapshot(&scheduled, "CANARYSCHEDedit").unwrap();
    let json = recorder
        .snapshot()
        .last()
        .cloned()
        .expect("editMessageText");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "editMessageText");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["message_id"], 70);
    assert_eq!(
        v["input_message_content"]["text"]["text"],
        "CANARYSCHEDedit"
    );
    assert!(!sink.rendered().contains("CANARYSCHED"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_delete_confirm_shape_and_tombstone() {
    use crate::composer::DeleteConfirm;

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
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewMessage","message":{"id":60,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"own outgoing","entities":[]}}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

    let incoming = DeleteConfirm {
        chat_id: ChatId(7),
        message_id: MessageId(50),
        revoke: false,
        can_revoke: false,
    };
    // M1: incoming messages are deletable for the current user
    // (`revoke: false`); the for-everyone toggle degrades to for-me.
    let extra = driver.delete_confirmed(&incoming).unwrap();
    let json = recorder.snapshot().last().cloned().expect("deleteMessages");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "deleteMessages");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["message_ids"], serde_json::json!([50]));
    assert_eq!(v["revoke"], false);

    let mut incoming_revoke = incoming.clone();
    incoming_revoke.revoke = true;
    let extra = driver.delete_confirmed(&incoming_revoke).unwrap();
    let json = recorder.snapshot().last().cloned().expect("deleteMessages");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["message_ids"], serde_json::json!([50]));
    assert_eq!(v["revoke"], false, "revoke is never sent for incoming");

    let missing = DeleteConfirm {
        chat_id: ChatId(7),
        message_id: MessageId(999),
        revoke: false,
        can_revoke: false,
    };
    assert_eq!(
        driver.delete_confirmed(&missing),
        Err(ConnectSendError::InvalidRequest)
    );

    // Selection mode: one request for several messages; "delete for
    // everyone" only when they're all your own.
    driver
        .delete_selected(ChatId(7), &[MessageId(50), MessageId(60)], true)
        .unwrap();
    let v: Value = serde_json::from_str(&recorder.snapshot().last().cloned().unwrap()).unwrap();
    assert_eq!(v["message_ids"], serde_json::json!([50, 60]));
    assert_eq!(v["revoke"], false, "a mixed selection deletes only for me");
    driver
        .delete_selected(ChatId(7), &[MessageId(60)], true)
        .unwrap();
    let v: Value = serde_json::from_str(&recorder.snapshot().last().cloned().unwrap()).unwrap();
    assert_eq!(v["revoke"], true);
    assert_eq!(
        driver.delete_selected(ChatId(7), &[MessageId(60), MessageId(999)], true),
        Err(ConnectSendError::InvalidRequest)
    );

    let confirm = DeleteConfirm::own(ChatId(7), MessageId(60), true, false).unwrap();
    let extra = driver.delete_confirmed(&confirm).unwrap();
    let json = recorder.snapshot().last().cloned().expect("deleteMessages");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "deleteMessages");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["message_ids"], serde_json::json!([60]));
    assert_eq!(v["revoke"], true);

    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateDeleteMessages","chat_id":7,"message_ids":[60],"is_permanent":true,"from_cache":false}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let history = driver.session.histories.get(&7).unwrap();
    assert!(!history.contains(MessageId(60)));
    assert!(history.is_tombstone(MessageId(60)));
    assert!(!sink.rendered().contains("CANARY"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_forward_messages_shape_and_dest_result() {
    use crate::composer::ForwardDraft;

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
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":8,"title":"Bob","type":{"@type":"chatTypePrivate","user_id":8},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatPosition","chat_id":8,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"8","is_pinned":false}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewMessage","message":{"id":60,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"own outgoing","entities":[]}}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

    let mut draft = ForwardDraft::from_message(ChatId(7), MessageId(50), false).unwrap();
    draft.toggle(ChatId(7), MessageId(60), false);
    let extra = driver.forward_messages(ChatId(8), &draft).unwrap();
    let json = recorder
        .snapshot()
        .last()
        .cloned()
        .expect("forwardMessages");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "forwardMessages");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["chat_id"], 8);
    assert_eq!(v["from_chat_id"], 7);
    assert_eq!(v["topic_id"], Value::Null);
    assert_eq!(v["message_ids"], serde_json::json!([50, 60]));
    assert_eq!(v["send_copy"], false);
    assert_eq!(v["remove_caption"], false);
    assert_eq!(v["options"], Value::Null);

    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"messages","@extra":"{}","total_count":2,"messages":[{{"id":80,"chat_id":8,"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hello already here","entities":[]}}}},"forward_info":{{"@type":"messageForwardInfo","origin":{{"@type":"messageOriginUser","sender_user_id":7}},"date":1}}}},{{"id":81,"chat_id":8,"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"own outgoing","entities":[]}}}},"forward_info":{{"@type":"messageForwardInfo","origin":{{"@type":"messageOriginUser","sender_user_id":7}},"date":1}}}}]}}"#,
                        extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let result = driver
        .session
        .last_forward
        .as_ref()
        .expect("forward result");
    assert_eq!(result.dest_title, "Bob");
    assert_eq!(result.forwarded_ids, vec![MessageId(80), MessageId(81)]);
    assert_eq!(result.success_label(), "Forwarded 2 messages to Bob");
    assert!(
        driver
            .session
            .histories
            .get(&8)
            .unwrap()
            .contains(MessageId(80))
    );
    assert_eq!(
        driver.session.forward_from_label(
            driver
                .session
                .histories
                .get(&8)
                .unwrap()
                .messages
                .get(&80)
                .unwrap()
                .forward_info
                .as_ref()
                .unwrap()
        ),
        "Forwarded from Alice"
    );
    assert!(!sink.rendered().contains("CANARY"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_add_and_remove_message_reaction_then_interaction_info() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    let extra = driver
        .toggle_message_reaction(ChatId(7), MessageId(50), "❤")
        .unwrap();
    let json = recorder
        .snapshot()
        .last()
        .cloned()
        .expect("addMessageReaction");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "addMessageReaction");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["message_id"], 50);
    assert_eq!(v["reaction_type"]["@type"], "reactionTypeEmoji");
    assert_eq!(v["reaction_type"]["emoji"], "❤");
    assert_eq!(v["is_big"], false);
    assert_eq!(v["update_recent_reactions"], true);
    assert!(!json.contains("setMessageReactions"));

    driver
        .ingest(
            copy_and_parse(
                &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateMessageInteractionInfo","chat_id":7,"message_id":50,"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"❤"},"total_count":1,"is_chosen":true,"used_sender_id":null,"recent_sender_ids":[]}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":false}}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let reacted = driver
        .session
        .histories
        .get(&7)
        .unwrap()
        .messages
        .get(&50)
        .unwrap();
    assert!(reacted.chosen_emoji("❤"));
    assert_eq!(
        reacted.emoji_reaction_chips()[0].chip_label().as_deref(),
        Some("❤ 1")
    );

    let remove_extra = driver
        .toggle_message_reaction(ChatId(7), MessageId(50), "❤")
        .unwrap();
    let remove_json = recorder
        .snapshot()
        .last()
        .cloned()
        .expect("removeMessageReaction");
    let v: Value = serde_json::from_str(&remove_json).unwrap();
    assert_eq!(v["@type"], "removeMessageReaction");
    assert_eq!(v["@extra"], remove_extra.0.to_string());
    assert_eq!(v["reaction_type"]["emoji"], "❤");

    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateMessageInteractionInfo","chat_id":7,"message_id":50,"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":null}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let cleared = driver
        .session
        .histories
        .get(&7)
        .unwrap()
        .messages
        .get(&50)
        .unwrap();
    assert!(cleared.emoji_reaction_chips().is_empty());
    assert!(!sink.rendered().contains("CANARY"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_edit_scheduled_message_sends_scheduling_state() {
    use crate::composer::ComposerScheduling;
    use crate::ids::{ChatId, MessageId};
    use crate::telegram::envelope::{
        MessageContent, MessageSchedulingState, ParsedMessage, TextContent,
    };

    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    driver.session.scheduled_messages.push(ParsedMessage {
        sender: None,
        id: MessageId(70),
        chat_id: ChatId(7),
        date: 0,
        is_outgoing: true,
        is_pinned: false,
        topic_id: None,
        thread_id: None,
        ephemeral: None,
        media_album_id: 0,
        author_signature: None,
        scheduling_state: Some(MessageSchedulingState::SendAtDate { send_date: 999 }),
        can_retry: false,
        send_state: Default::default(),
        content: MessageContent::Text(TextContent::plain("later")),
        files: Vec::new(),
        reply_to: None,
        forward_info: None,
        extras: Default::default(),
        interaction_info: None,
        reply_markup: None,
        self_destruct: None,
        auto_delete: None,
    });
    // Unknown ids are refused before anything is sent.
    assert_eq!(
        driver.edit_scheduled_message(ChatId(7), MessageId(71), ComposerScheduling::None),
        Err(ConnectSendError::InvalidRequest)
    );
    let extra = driver
        .edit_scheduled_message(
            ChatId(7),
            MessageId(70),
            ComposerScheduling::SendAtDate(1_800_000_600),
        )
        .unwrap();
    let json = recorder.snapshot().last().cloned().expect("request");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "editMessageSchedulingState");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["scheduling_state"]["send_date"], 1_800_000_600);
    let _ = std::fs::remove_dir_all(&dir);
}
