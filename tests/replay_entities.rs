//! Text entities and photo viewer replay tests.
//! Split from `tests/replay.rs` — pure code motion.
mod replay_common;
use replay_common::*;

#[test]
fn replay_text_entities_mixed_nested_unknown_and_malformed() {
    use quill::text::{TextEntityKind, styled_runs, utf8_to_utf16_offset};
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);

    let text = "Bold italic bolditalic underline strike secret code\nfn f() {}";
    let span = |needle: &str| -> (i32, i32) {
        let start = text.find(needle).unwrap();
        let s = utf8_to_utf16_offset(text, start).unwrap();
        let e = utf8_to_utf16_offset(text, start + needle.len()).unwrap();
        (s, e - s)
    };
    let ent = |needle: &str, type_json: &str| -> String {
        let (offset, length) = span(needle);
        format!(
            r#"{{"@type":"textEntity","offset":{offset},"length":{length},"type":{type_json}}}"#
        )
    };
    let entities = [
        ent("Bold", r#"{"@type":"textEntityTypeBold"}"#),
        ent("italic", r#"{"@type":"textEntityTypeItalic"}"#),
        ent("bolditalic", r#"{"@type":"textEntityTypeBold"}"#),
        ent("bolditalic", r#"{"@type":"textEntityTypeItalic"}"#),
        ent("underline", r#"{"@type":"textEntityTypeUnderline"}"#),
        ent("strike", r#"{"@type":"textEntityTypeStrikethrough"}"#),
        ent("secret", r#"{"@type":"textEntityTypeSpoiler"}"#),
        ent("code", r#"{"@type":"textEntityTypeCode"}"#),
        ent(
            "fn f() {}",
            r#"{"@type":"textEntityTypePreCode","language":"rust"}"#,
        ),
        // Unknown types are ignored, never crash the parse.
        ent("Bold", r#"{"@type":"textEntityTypeFutureThing"}"#),
        // textEntityTypeBlockQuote is parsed (kept) since the blockquote slice.
        ent("Bold", r#"{"@type":"textEntityTypeBlockQuote"}"#),
        ent(
            "Bold",
            r#"{"@type":"textEntityTypeCustomEmoji","custom_emoji_id":"123"}"#,
        ),
        // Malformed offsets are dropped (zero length, past the end).
        r#"{"@type":"textEntity","offset":5,"length":0,"type":{"@type":"textEntityTypeBold"}}"#
            .to_string(),
        r#"{"@type":"textEntity","offset":500,"length":10,"type":{"@type":"textEntityTypeBold"}}"#
            .to_string(),
    ]
    .join(",");
    let text_json = serde_json::to_string(text).unwrap();
    let json = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":30,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{text_json},"entities":[{entities}]}}}}}}}}"#
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
            &json,
        ],
    );
    let message = session
        .histories
        .get(&7)
        .unwrap()
        .messages
        .get(&30)
        .unwrap();
    let quill::telegram::envelope::MessageContent::Text(content) = &message.content else {
        panic!("expected text");
    };
    assert_eq!(content.text, text);
    let kinds: Vec<&TextEntityKind> = content.entities.iter().map(|e| &e.kind).collect();
    assert_eq!(kinds.len(), 11, "unknown + malformed entities are dropped");
    assert!(matches!(kinds[0], TextEntityKind::Bold));
    assert!(matches!(kinds[1], TextEntityKind::Italic));
    assert!(matches!(kinds[2], TextEntityKind::Bold));
    assert!(matches!(kinds[3], TextEntityKind::Italic));
    assert!(matches!(kinds[4], TextEntityKind::Underline));
    assert!(matches!(kinds[5], TextEntityKind::Strikethrough));
    assert!(matches!(kinds[6], TextEntityKind::Spoiler));
    assert!(matches!(kinds[7], TextEntityKind::Code));
    assert!(matches!(
        kinds[8],
        TextEntityKind::PreCode { language } if language == "rust"
    ));
    // Block quotes are parsed since the blockquote slice (input order kept).
    assert!(matches!(kinds[9], TextEntityKind::BlockQuote));
    // Custom emoji is parsed since the custom-emoji-in-text slice.
    assert!(matches!(
        kinds[10],
        TextEntityKind::CustomEmoji {
            custom_emoji_id: 123
        }
    ));

    // Nesting combines: the "bolditalic" span renders bold italic.
    let runs = styled_runs(&content.text, &content.entities);
    let nested = runs
        .iter()
        .find(|run| run.text == "bolditalic")
        .expect("nested run");
    assert!(nested.style.bold && nested.style.italic);
    // Pre keeps its language; spoiler and code spans split correctly.
    let pre = runs
        .iter()
        .find(|run| run.text == "fn f() {}")
        .expect("pre run");
    assert!(pre.style.pre);
    assert_eq!(pre.style.language.as_deref(), Some("rust"));
    let spoiler = runs
        .iter()
        .find(|run| run.text == "secret")
        .expect("spoiler run");
    assert!(spoiler.style.spoiler);
    let code = runs
        .iter()
        .find(|run| run.text == "code")
        .expect("code run");
    assert!(code.style.code);
}

#[test]
fn replay_photo_caption_carries_entities() {
    use quill::text::{TextEntityKind, styled_runs};
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    let caption = "cap bold plus link https://example.com/x";
    let link_at = caption.find("https").unwrap();
    let file = r#"{"@type":"file","id":11,"size":10,"expected_size":10,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"CANARY_REMOTE","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":10}}"#;
    let caption_json = serde_json::to_string(caption).unwrap();
    let json = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":31,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{file},"width":320,"height":240,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":{caption_json},"entities":[{{"@type":"textEntity","offset":4,"length":4,"type":{{"@type":"textEntityTypeBold"}}}},{{"@type":"textEntity","offset":{link_at},"length":21,"type":{{"@type":"textEntityTypeUrl"}}}}]}},"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
            &json,
        ],
    );
    let message = session
        .histories
        .get(&7)
        .unwrap()
        .messages
        .get(&31)
        .unwrap();
    let quill::telegram::envelope::MessageContent::Photo(photo) = &message.content else {
        panic!("expected photo");
    };
    assert_eq!(photo.caption, caption);
    assert_eq!(photo.caption_entities.len(), 2);
    assert!(matches!(
        photo.caption_entities[0].kind,
        TextEntityKind::Bold
    ));
    assert!(matches!(
        photo.caption_entities[1].kind,
        TextEntityKind::Url
    ));
    let runs = styled_runs(&photo.caption, &photo.caption_entities);
    let bold = runs
        .iter()
        .find(|run| run.text == "bold")
        .expect("bold run");
    assert!(bold.style.bold);
    let link = runs
        .iter()
        .find(|run| run.text == "https://example.com/x")
        .expect("link run");
    assert_eq!(link.href.as_deref(), Some("https://example.com/x"));
    assert!(!sink.rendered().contains("CANARY_REMOTE"));
}

#[test]
fn replay_click_photo_opens_viewer_with_largest_file_id() {
    // Phase 4.5: clicking a photo message opens the viewer on the clicked
    // message's item, with the largest size's file id as the
    // download/display target.
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn quill::diagnostics::DiagnosticSink> = sink.clone();
    let mut session = Session::new(AccountKey::primary(), dyn_sink);
    let seq = AtomicU64::new(0);
    let file = |id: i32| {
        format!(
            r#"{{"@type":"file","id":{id},"size":10,"expected_size":10,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"CANARY_REMOTE","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":10}}}}"#
        )
    };
    let sizes = format!(
        "{{\"@type\":\"photoSize\",\"type\":\"m\",\"photo\":{thumb},\"width\":320,\"height\":240,\"progressive_sizes\":[]}},{{\"@type\":\"photoSize\",\"type\":\"x\",\"photo\":{full},\"width\":1280,\"height\":960,\"progressive_sizes\":[]}}",
        thumb = file(1),
        full = file(2),
    );
    let photo_message = format!(
        "{{\"@type\":\"updateNewMessage\",\"message\":{{\"id\":20,\"chat_id\":7,\"is_outgoing\":false,\"content\":{{\"@type\":\"messagePhoto\",\"photo\":{{\"@type\":\"photo\",\"has_stickers\":false,\"sizes\":[{sizes}]}},\"caption\":{{\"@type\":\"formattedText\",\"text\":\"CANARY_REPLAY_viewer\",\"entities\":[]}},\"has_spoiler\":false,\"is_secret\":false}}}}}}",
    );
    apply_all_seq(
        &mut session,
        &sink,
        &seq,
        &[
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
            &photo_message,
        ],
    );
    let history = session.histories.get(&7).expect("chat history");
    let messages: Vec<quill::state::HistoryMessage> =
        history.ordered().into_iter().cloned().collect();
    // The click handler builds the viewer list from the chat's media
    // messages, then opens on the clicked message's index.
    let items = quill::media_viewer::collect_media_items(&messages);
    assert_eq!(items.len(), 1);
    let index = items
        .iter()
        .position(|item| item.message_id == quill::ids::MessageId(20))
        .expect("clicked photo is in the viewer list");
    let viewer = quill::media_viewer::MediaViewer::open(items, index);
    let current = viewer.current().expect("viewer is open");
    assert_eq!(current.download_file_id, quill::ids::FileId(2));
    assert_eq!(current.display_file_ids[0], quill::ids::FileId(2));
    assert!(current.display_file_ids.contains(&quill::ids::FileId(1)));
    assert_eq!(current.caption, "CANARY_REPLAY_viewer");
    assert!(!sink.rendered().contains("CANARY_REPLAY"));
    assert!(!sink.rendered().contains("CANARY_REMOTE"));
}
