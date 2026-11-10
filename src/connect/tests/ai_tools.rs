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

    driver.session.note_ai_ask(
        extra,
        ChatId(7),
        crate::composer::DraftTopic::Chat,
        "teh draft".into(),
    );
    let json = format!(
        r#"{{"@type":"fixedText","text":{{"@type":"formattedText","text":"the draft","entities":[]}},"diff_text":{{"@type":"diffText","text":"teh → the","entities":[]}},"@extra":"{id}"}}"#,
        id = extra.0,
    );
    let owned = copy_and_parse(&json, &seq, &dyn_sink).expect("parse fixedText");
    driver.ingest(owned).expect("ingest fixedText");
    let answers = driver.session.take_ai_answers_for(Some(ChatId(7)));
    assert_eq!(answers.len(), 1);
    assert_eq!(answers[0].ask.chat_id, ChatId(7));
    assert_eq!(answers[0].ask.source, "teh draft");
    assert_eq!(
        answers[0].body,
        crate::state::AiBody::Text("the draft".into())
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
    assert_eq!(v["language_code"], "en");
    assert_eq!(v["add_emojis"], false);

    driver.session.note_ai_ask(
        extra,
        ChatId(7),
        crate::composer::DraftTopic::Chat,
        "haiku about rain".into(),
    );
    let json = format!(
        r#"{{"@type":"richMessage","is_full":true,"is_rtl":false,"blocks":[{{"@type":"pageBlockParagraph","text":{{"@type":"richTextPlain","text":"rain falls"}}}},{{"@type":"pageBlockDivider"}}],"@extra":"{id}"}}"#,
        id = extra.0,
    );
    let owned = copy_and_parse(&json, &seq, &dyn_sink).expect("parse richMessage");
    driver.ingest(owned).expect("ingest richMessage");
    let mut answers = driver.session.take_ai_answers_for(Some(ChatId(7)));
    let answer = answers.pop().expect("blocks stored");
    assert_eq!(answer.ask.chat_id, ChatId(7));
    let crate::state::AiBody::Blocks(rich, note) = answer.body else {
        panic!("a rich answer")
    };
    assert_eq!(note, "AI created the draft");
    assert!(rich.is_full);
    assert!(matches!(
        rich.blocks.as_slice(),
        [RichBlock::Paragraph { text, .. }, RichBlock::Divider] if text == "rain falls"
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn create_rich_message_with_ai_sends_user_language_code() {
    let (dir, mut driver, recorder, _sink, _dyn_sink, _seq) = ai_harness();
    driver.session.settings.language_prefs.system_language_code = "es".to_string();
    driver
        .create_rich_message_with_ai(ChatId(7), "haiku about rain")
        .expect("create request");
    let v = sent_request(&recorder, "createRichMessageWithAi");
    assert_eq!(v["language_code"], "es");
    let _ = std::fs::remove_dir_all(&dir);
}

fn blocks_note(answer: &crate::state::AiAnswer) -> &'static str {
    match &answer.body {
        crate::state::AiBody::Blocks(_, note) => note,
        crate::state::AiBody::Text(_) => "",
    }
}

fn ingest_rich_paragraph(
    driver: &mut ConnectDriver<Arc<RecordingSender>>,
    dyn_sink: &Arc<dyn DiagnosticSink>,
    seq: &AtomicU64,
    extra: crate::ids::RequestId,
) {
    driver.session.note_ai_ask(
        extra,
        ChatId(7),
        crate::composer::DraftTopic::Chat,
        String::new(),
    );
    let json = format!(
        r#"{{"@type":"richMessage","is_full":true,"is_rtl":false,"blocks":[{{"@type":"pageBlockParagraph","text":{{"@type":"richTextPlain","text":"rain falls"}}}}],"@extra":"{id}"}}"#,
        id = extra.0,
    );
    let owned = copy_and_parse(&json, seq, dyn_sink).expect("parse richMessage");
    driver.ingest(owned).expect("ingest richMessage");
}

#[test]
fn rich_ai_draft_notes_match_the_method() {
    let (dir, mut driver, _recorder, _sink, dyn_sink, seq) = ai_harness();
    let fixed = driver
        .fix_rich_message_with_ai(ChatId(7), &[RichBlock::Divider])
        .expect("fix rich request");
    ingest_rich_paragraph(&mut driver, &dyn_sink, &seq, fixed);
    assert_eq!(
        blocks_note(&driver.session.take_ai_answers_for(Some(ChatId(7)))[0]),
        "AI fixed the draft"
    );

    let rewritten = driver
        .compose_rich_message_with_ai(ChatId(7), &[RichBlock::Divider])
        .expect("rewrite rich request");
    ingest_rich_paragraph(&mut driver, &dyn_sink, &seq, rewritten);
    assert_eq!(
        blocks_note(&driver.session.take_ai_answers_for(Some(ChatId(7)))[0]),
        "AI rewrote the draft"
    );
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
    assert_invalid(driver.compose_rich_message_with_ai(ChatId(31), &[RichBlock::Divider]));
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
    let err = driver
        .session
        .messages
        .ai_error
        .take_all()
        .into_iter()
        .next()
        .expect("AI error stored");
    assert!(err.contains("Premium"), "premium line surfaced, got: {err}");
    // Never applied as a success.
    assert!(
        driver
            .session
            .take_ai_answers_for(Some(ChatId(7)))
            .is_empty()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_ai_answer_is_matched_to_its_ask_and_a_newer_ask_on_the_slot_replaces_it() {
    use crate::composer::DraftTopic;
    let (dir, mut driver, _recorder, _sink, dyn_sink, seq) = ai_harness();
    let first = driver.fix_text_with_ai(ChatId(7), "one").unwrap();
    driver
        .session
        .note_ai_ask(first, ChatId(7), DraftTopic::Chat, "one".into());
    let second = driver.fix_text_with_ai(ChatId(7), "two").unwrap();
    driver
        .session
        .note_ai_ask(second, ChatId(7), DraftTopic::Chat, "two".into());
    // A second ask for another topic of the same chat is its own.
    let other = driver.fix_text_with_ai(ChatId(7), "three").unwrap();
    driver
        .session
        .note_ai_ask(other, ChatId(7), DraftTopic::Forum(3), "three".into());
    for (request, text) in [(first, "ONE"), (second, "TWO"), (other, "THREE")] {
        let json = format!(
            r#"{{"@type":"fixedText","text":{{"@type":"formattedText","text":"{text}","entities":[]}},"diff_text":{{"@type":"diffText","text":"x","entities":[]}},"@extra":"{id}"}}"#,
            id = request.0,
        );
        driver
            .ingest(copy_and_parse(&json, &seq, &dyn_sink).unwrap())
            .unwrap();
    }
    let answers = driver.session.take_ai_answers_for(Some(ChatId(7)));
    let bodies: Vec<_> = answers.iter().map(|a| a.ask.source.as_str()).collect();
    // The replaced ask's answer is dropped: it would land on a moved-on draft.
    assert_eq!(bodies, ["two", "three"]);
    let _ = std::fs::remove_dir_all(&dir);
}
