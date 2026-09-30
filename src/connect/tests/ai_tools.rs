//! Connect-driver tests: rich-text composer AI tools (`fixTextWithAi`,
//! `composeTextWithAi`, `composeRichMessageWithAi`,
//! `createRichMessageWithAi`, `fixRichMessageWithAi`).
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::{AccountKey, ChatId};
use crate::platform::MemorySecretStore;
use crate::rich::RichBlock;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

type AiHarness = (
    std::path::PathBuf,
    ConnectDriver<Arc<RecordingSender>>,
    Arc<RecordingSender>,
    Arc<MemorySink>,
    Arc<dyn DiagnosticSink>,
    AtomicU64,
);

fn ai_harness() -> AiHarness {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    (dir, driver, recorder, sink, dyn_sink, seq)
}

#[test]
fn fix_text_with_ai_applies_fixed_text() {
    let (dir, mut driver, recorder, _sink, dyn_sink, seq) = ai_harness();
    let extra = driver
        .fix_text_with_ai(ChatId(7), "teh draft")
        .expect("fix request");
    let v = sent_request(&recorder, "fixTextWithAi");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["text"]["@type"], "formattedText");
    assert_eq!(v["text"]["text"], "teh draft");

    let json = format!(
        r#"{{"@type":"fixedText","text":{{"@type":"formattedText","text":"the draft","entities":[]}},"diff_text":{{"@type":"diffText","text":"teh → the","entities":[]}},"@extra":"{id}"}}"#,
        id = extra.0,
    );
    let owned = copy_and_parse(&json, &seq, &dyn_sink).expect("parse fixedText");
    driver.ingest(owned).expect("ingest fixedText");
    assert_eq!(
        driver.session.ai_composer_text,
        Some((ChatId(7), "the draft".to_string()))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn create_rich_message_with_ai_round_trips_blocks() {
    let (dir, mut driver, recorder, _sink, dyn_sink, seq) = ai_harness();
    let extra = driver
        .create_rich_message_with_ai(ChatId(7), "haiku about rain")
        .expect("create request");
    let v = sent_request(&recorder, "createRichMessageWithAi");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["prompt"], "haiku about rain");
    assert_eq!(v["language_code"], "");
    assert_eq!(v["add_emojis"], false);

    let json = format!(
        r#"{{"@type":"richMessage","is_full":true,"is_rtl":false,"blocks":[{{"@type":"pageBlockParagraph","text":{{"@type":"richTextPlain","text":"rain falls"}}}},{{"@type":"pageBlockDivider"}}],"@extra":"{id}"}}"#,
        id = extra.0,
    );
    let owned = copy_and_parse(&json, &seq, &dyn_sink).expect("parse richMessage");
    driver.ingest(owned).expect("ingest richMessage");
    let (chat_id, rich) = driver.session.ai_composer_blocks.expect("blocks stored");
    assert_eq!(chat_id, ChatId(7));
    assert!(rich.is_full);
    assert!(matches!(
        rich.blocks.as_slice(),
        [RichBlock::Paragraph { text, .. }, RichBlock::Divider] if text == "rain falls"
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn compose_text_with_ai_uses_honest_defaults() {
    let (dir, mut driver, recorder, _sink, _dyn_sink, _seq) = ai_harness();
    let extra = driver
        .compose_text_with_ai(ChatId(7), "draft")
        .expect("compose request");
    let v = sent_request(&recorder, "composeTextWithAi");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["text"]["text"], "draft");
    assert_eq!(v["translate_to_language_code"], "");
    assert_eq!(v["style_name"], "");
    assert_eq!(v["add_emojis"], false);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fix_rich_message_with_ai_sends_input_rich_message() {
    let (dir, mut driver, recorder, _sink, _dyn_sink, _seq) = ai_harness();
    let blocks = [RichBlock::Divider];
    let extra = driver
        .fix_rich_message_with_ai(ChatId(7), &blocks)
        .expect("fix rich request");
    let v = sent_request(&recorder, "fixRichMessageWithAi");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["message"]["@type"], "inputRichMessage");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ai_tools_refused_in_secret_chat() {
    let (dir, mut driver, _recorder, _sink, dyn_sink, seq) = ai_harness();
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateNewChat","chat":{"id":31,"title":"Secret","type":{"@type":"chatTypeSecret","secret_chat_id":31,"user_id":7},"unread_count":0}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_invalid(driver.fix_text_with_ai(ChatId(31), "draft"));
    assert_invalid(driver.compose_text_with_ai(ChatId(31), "draft"));
    assert_invalid(driver.create_rich_message_with_ai(ChatId(31), "prompt"));
    assert_invalid(driver.fix_rich_message_with_ai(ChatId(31), &[RichBlock::Divider]));
    assert_invalid(
        driver.compose_rich_message_with_ai(ChatId(31), &[RichBlock::Divider]),
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ai_tools_refuse_empty_draft() {
    let (dir, mut driver, _recorder, _sink, _dyn_sink, _seq) = ai_harness();
    assert_invalid(driver.fix_text_with_ai(ChatId(7), "   "));
    assert_invalid(driver.compose_text_with_ai(ChatId(7), ""));
    assert_invalid(driver.create_rich_message_with_ai(ChatId(7), ""));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ai_flood_premium_error_surfaces_premium_line() {
    let (dir, mut driver, _recorder, _sink, dyn_sink, seq) = ai_harness();
    let extra = driver
        .fix_text_with_ai(ChatId(7), "teh draft")
        .expect("fix request");
    let json = format!(
        r#"{{"@type":"error","code":400,"message":"AICOMPOSE_FLOOD_PREMIUM","@extra":"{id}"}}"#,
        id = extra.0,
    );
    let owned = copy_and_parse(&json, &seq, &dyn_sink).expect("parse error");
    driver.ingest(owned).expect("ingest error");
    let err = driver.session.ai_error.expect("AI error stored");
    assert!(
        err.contains("Premium"),
        "premium line surfaced, got: {err}"
    );
    // Never applied as a success.
    assert!(driver.session.ai_composer_text.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
