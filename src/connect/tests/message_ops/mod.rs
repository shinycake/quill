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

mod edits;
mod group_admin;

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
        driver.session.search.chat_search.jump,
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
    driver.session.messages.message_caption_length_max = 4;

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
    driver.session.messages.message_caption_length_max = 7;
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
    driver.session.messages.message_caption_length_max = 4;

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
    driver.session.messages.message_caption_length_max = 4;

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
    driver.session.messages.message_text_length_max = 5;

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
    // `GetWebPageInstantView` request populates `session.messages.instant_view`
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
        .messages
        .instant_view_urls
        .insert(extra, "https://example.com/article".to_string());

    let json = format!(
        r#"{{"@type":"webPageInstantView","page_blocks":[],"@extra":"{id}"}}"#,
        id = extra.0,
    );
    let owned = copy_and_parse(&json, &seq, &dyn_sink).expect("parse IV");
    driver.ingest(owned).expect("ingest IV");
    let iv = driver
        .session
        .messages
        .instant_view
        .as_ref()
        .expect("IV stored");
    assert_eq!(iv.url, "https://example.com/article");
    // A success is never stashed as a browser fallback.
    assert!(driver.session.messages.instant_view_fallback_url.is_none());
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
        .messages
        .instant_view_urls
        .insert(extra, "https://example.com/noiv".to_string());

    let json = format!(
        r#"{{"@type":"error","code":404,"message":"Not Found","@extra":"{id}"}}"#,
        id = extra.0,
    );
    let owned = copy_and_parse(&json, &seq, &dyn_sink).expect("parse error");
    driver.ingest(owned).expect("ingest error");
    assert!(driver.session.messages.instant_view.is_none());
    assert_eq!(
        driver.session.messages.instant_view_fallback_url.as_deref(),
        Some("https://example.com/noiv")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_link_preview_prefetch_ingest() {
    // MED4b: a `linkPreview` answer for a tracked `GetLinkPreview`
    // request populates `session.messages.composer_preview` (the chip reads
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
        .messages
        .composer_preview_urls
        .insert(extra, "https://example.com/story".to_string());
    driver.session.messages.composer_preview = Some(ComposerLinkPreview {
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
        .messages
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
        .messages
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
        .messages
        .composer_preview
        .as_ref()
        .expect("preview kept");
    assert_eq!(stored.url, "https://example.com/story");

    // 404 for the current URL → "no link info", never a card.
    let miss = driver.session.request(RequestPurpose::GetLinkPreview, None);
    driver
        .session
        .messages
        .composer_preview_urls
        .insert(miss, "https://example.com/story".to_string());
    driver.session.messages.composer_preview = Some(ComposerLinkPreview {
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
        .messages
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
