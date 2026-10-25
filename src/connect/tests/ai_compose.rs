//! Connect-driver tests: the AI compose box, the apply-in-place shortcut
//! and the text composition style requests.
use super::super::*;
use super::*;
use crate::ai_compose::{
    AiBoxError, AiRequest, DiffSegment, DiffSegmentKind, StyleLimitNotice, diff_segments,
};
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::{AccountKey, ChatId, RequestId};
use crate::platform::MemorySecretStore;
use crate::state::{Session, StyleError, StyleNotice};
use crate::telegram::client::copy_and_parse;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

struct Harness {
    dir: std::path::PathBuf,
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    sink: Arc<dyn DiagnosticSink>,
    seq: AtomicU64,
}

impl Harness {
    fn new() -> Self {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &sink);
        Self {
            dir,
            driver,
            recorder,
            sink,
            seq,
        }
    }

    fn feed(&mut self, value: Value) {
        let owned = copy_and_parse(&value.to_string(), &self.seq, &self.sink).expect("parse");
        self.driver.ingest(owned).expect("ingest");
    }

    fn answer(&mut self, extra: RequestId, mut value: Value) {
        value["@extra"] = json!(extra.as_extra());
        self.feed(value);
    }

    fn fail(&mut self, extra: RequestId, message: &str) {
        self.answer(
            extra,
            json!({"@type": "error", "code": 400, "message": message}),
        );
    }

    fn option(&mut self, name: &str, value: i64) {
        self.feed(json!({
            "@type": "updateOption",
            "name": name,
            "value": {"@type": "optionValueInteger", "value": value.to_string()}
        }));
    }

    fn sent(&self, type_name: &str) -> Value {
        sent_request(&self.recorder, type_name)
    }

    fn ai(&self) -> &crate::state::AiComposeState {
        &self.driver.session.messages.ai_compose
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn style_json(name: &str, is_custom: bool) -> Value {
    json!({
        "@type": "textCompositionStyle",
        "name": name,
        "custom_emoji_id": "0",
        "title": name,
        "is_custom": is_custom,
        "is_creator": false,
        "install_count": 0,
        "prompt": "",
        "creator_user_id": 0,
        "english_example": null
    })
}

fn compose(text: &str, translate_to: &str, style_name: &str, add_emojis: bool) -> AiRequest {
    AiRequest::Compose {
        text: text.into(),
        translate_to: translate_to.into(),
        style_name: style_name.into(),
        add_emojis,
    }
}

#[test]
fn box_requests_send_compose_and_fix() {
    let mut h = Harness::new();
    let extra = h
        .driver
        .ai_box_request(ChatId(7), &compose("Hallo", "en", "formal", true), 11)
        .expect("compose");
    let v = h.sent("composeTextWithAi");
    assert_eq!(v["@extra"], extra.as_extra());
    assert_eq!(v["text"]["@type"], "formattedText");
    assert_eq!(v["text"]["text"], "Hallo");
    assert_eq!(v["translate_to_language_code"], "en");
    assert_eq!(v["style_name"], "formal");
    assert_eq!(v["add_emojis"], true);

    let extra = h
        .driver
        .ai_box_request(
            ChatId(7),
            &AiRequest::Fix {
                text: "teh draft".into(),
            },
            12,
        )
        .expect("fix");
    let v = h.sent("fixTextWithAi");
    assert_eq!(v["@extra"], extra.as_extra());
    assert_eq!(v["text"]["text"], "teh draft");
}

#[test]
fn box_requests_are_refused_when_empty_or_secret() {
    let mut h = Harness::new();
    assert_invalid(
        h.driver
            .ai_box_request(ChatId(7), &compose("  ", "en", "", false), 1),
    );
    assert_invalid(h.driver.ai_apply_in_place(ChatId(7), "text", "", 2));
    h.feed(
        json!({"@type":"updateNewChat","chat":{"id":31,"title":"Secret",
        "type":{"@type":"chatTypeSecret","secret_chat_id":31,"user_id":7},"unread_count":0}}),
    );
    assert_invalid(
        h.driver
            .ai_box_request(ChatId(31), &compose("hi", "en", "", false), 3),
    );
    assert_invalid(h.driver.ai_apply_in_place(ChatId(31), "hi", "pirate", 4));
}

#[test]
fn style_requests_send_every_field() {
    let mut h = Harness::new();
    let extra = h
        .driver
        .create_text_composition_style("Pirate", 77, "talk like a pirate", true)
        .unwrap();
    let v = h.sent("createTextCompositionStyle");
    assert_eq!(v["@extra"], extra.as_extra());
    assert_eq!(v["title"], "Pirate");
    assert_eq!(v["custom_emoji_id"], "77");
    assert_eq!(v["prompt"], "talk like a pirate");
    assert_eq!(v["show_creator"], true);

    h.driver
        .edit_text_composition_style("pirate", "Pirate 2", 0, "arr", false)
        .unwrap();
    let v = h.sent("editTextCompositionStyle");
    assert_eq!(v["name"], "pirate");
    assert_eq!(v["title"], "Pirate 2");
    assert_eq!(v["custom_emoji_id"], "0");
    assert_eq!(v["prompt"], "arr");
    assert_eq!(v["show_creator"], false);

    for (method, send) in [
        (
            "deleteTextCompositionStyle",
            ConnectDriver::delete_text_composition_style as fn(&mut _, &str) -> _,
        ),
        (
            "removeTextCompositionStyle",
            ConnectDriver::remove_text_composition_style,
        ),
        (
            "addTextCompositionStyle",
            ConnectDriver::add_text_composition_style,
        ),
    ] {
        let extra = send(&mut h.driver, "pirate").unwrap();
        let v = h.sent(method);
        assert_eq!(v["@extra"], extra.as_extra(), "{method}");
        assert_eq!(v["name"], "pirate", "{method}");
    }

    h.driver.search_text_composition_style("pirate", 5).unwrap();
    assert_eq!(h.sent("searchTextCompositionStyle")["name"], "pirate");
    h.driver
        .get_text_composition_style_example("pirate", 2, 6)
        .unwrap();
    let v = h.sent("getTextCompositionStyleExample");
    assert_eq!(v["name"], "pirate");
    assert_eq!(v["example_number"], 2);

    let extra = h
        .driver
        .ai_apply_in_place(ChatId(7), "selected text", "pirate", 9)
        .unwrap();
    let v = h.sent("composeTextWithAi");
    assert_eq!(v["@extra"], extra.as_extra());
    assert_eq!(v["text"]["text"], "selected text");
    assert_eq!(v["translate_to_language_code"], "");
    assert_eq!(v["style_name"], "pirate");
    assert_eq!(v["add_emojis"], false);
}

#[test]
fn style_requests_respect_the_option_limits() {
    let mut h = Harness::new();
    assert_invalid(
        h.driver
            .create_text_composition_style("", 0, "prompt", false),
    );
    assert_invalid(
        h.driver
            .create_text_composition_style("Title", 0, " ", false),
    );
    assert_invalid(h.driver.delete_text_composition_style(""));
    assert_invalid(h.driver.get_text_composition_style_example("pirate", -1, 1));
    h.option("text_composition_style_title_length_max", 5);
    h.option("text_composition_style_prompt_length_max", 10);
    h.option("text_composition_style_example_count", 3);
    h.option("added_text_composition_style_count_max", 5);
    let limits = h.ai().limits;
    assert_eq!(limits.title_length_max, Some(5));
    assert_eq!(limits.prompt_length_max, Some(10));
    assert_eq!(limits.example_count, Some(3));
    assert_eq!(limits.added_count_max, Some(5));
    assert_invalid(
        h.driver
            .create_text_composition_style("Pirates", 0, "arr", false),
    );
    assert_invalid(
        h.driver
            .edit_text_composition_style("pirate", "Arr", 0, "far too long", false),
    );
    assert!(
        h.driver
            .create_text_composition_style("Arr😀", 0, "arr", false)
            .is_ok()
    );
    assert_invalid(h.driver.get_text_composition_style_example("pirate", 3, 1));
    assert!(
        h.driver
            .get_text_composition_style_example("pirate", 2, 1)
            .is_ok()
    );
}

#[test]
fn style_list_update_populates_the_tabs() {
    let mut h = Harness::new();
    assert!(!h.ai().styles_received);
    h.feed(json!({
        "@type": "updateTextCompositionStyles",
        "styles": [style_json("formal", false), style_json("pirate", true)]
    }));
    assert!(h.ai().styles_received);
    assert_eq!(h.ai().styles.names(), ["formal", "pirate"]);
}

#[test]
fn fixed_text_for_the_box_lands_with_its_diff() {
    let mut h = Harness::new();
    let extra = h
        .driver
        .ai_box_request(
            ChatId(7),
            &AiRequest::Fix {
                text: "teh draft".into(),
            },
            21,
        )
        .unwrap();
    h.answer(
        extra,
        json!({
            "@type": "fixedText",
            "text": {"@type": "formattedText", "text": "the draft", "entities": []},
            "diff_text": {"@type": "diffText", "text": "the draft", "entities": [
                {"@type": "diffEntity", "offset": 0, "length": 3,
                 "type": {"@type": "diffEntityTypeReplace", "old_text": "teh"}}
            ]}
        }),
    );
    let answer = h.ai().box_answer.clone().expect("box answer");
    assert_eq!(answer.token, 21);
    let result = answer.outcome.expect("fixed");
    assert_eq!(result.text, "the draft");
    let diff = result.diff.expect("diff");
    assert_eq!(
        diff_segments(&diff),
        [
            DiffSegment {
                text: "the".into(),
                kind: DiffSegmentKind::Replaced {
                    old_text: "teh".into()
                }
            },
            DiffSegment {
                text: " draft".into(),
                kind: DiffSegmentKind::Same
            },
        ]
    );
    assert!(h.driver.session.messages.ai_composer_text.is_none());
}

#[test]
fn formatted_text_for_the_box_never_touches_the_draft() {
    let mut h = Harness::new();
    let extra = h
        .driver
        .ai_box_request(ChatId(7), &compose("Hallo", "en", "", false), 31)
        .unwrap();
    h.answer(
        extra,
        json!({"@type": "formattedText", "text": "Hello", "entities": [
            {"@type": "textEntity", "offset": 0, "length": 5, "type": {"@type": "textEntityTypeBold"}}
        ]}),
    );
    let answer = h.ai().box_answer.clone().expect("box answer");
    assert_eq!(answer.token, 31);
    let result = answer.outcome.expect("composed");
    assert_eq!(result.text, "Hello");
    assert_eq!(result.entities.len(), 1);
    assert!(result.diff.is_none());
    assert!(h.driver.session.messages.ai_composer_text.is_none());
    assert!(h.ai().apply_in_place.is_none());
}

#[test]
fn box_errors_map_to_box_error_kinds() {
    let mut h = Harness::new();
    for (message, expected) in [
        ("AICOMPOSE_FLOOD_PREMIUM", AiBoxError::FloodPremium),
        ("INPUT_TEXT_TOO_LONG", AiBoxError::TooLong),
        (
            "AICOMPOSE_TONE_INVALID",
            AiBoxError::StyleInvalid("AICOMPOSE_TONE_INVALID".into()),
        ),
        ("SOMETHING_ELSE", AiBoxError::Other("SOMETHING_ELSE".into())),
    ] {
        let extra = h
            .driver
            .ai_box_request(ChatId(7), &compose("Hallo", "en", "", false), 41)
            .unwrap();
        h.fail(extra, message);
        let answer = h.ai().box_answer.clone().expect("box answer");
        assert_eq!(answer.token, 41);
        assert_eq!(answer.outcome, Err(expected), "{message}");
    }
    assert!(h.driver.session.messages.ai_error.is_none());
}

#[test]
fn apply_in_place_answers_and_style_errors() {
    let mut h = Harness::new();
    let extra = h
        .driver
        .ai_apply_in_place(ChatId(7), "hello", "pirate", 51)
        .unwrap();
    h.answer(
        extra,
        json!({"@type": "formattedText", "text": "ahoy", "entities": []}),
    );
    let answer = h.ai().apply_in_place.clone().expect("answer");
    assert_eq!(answer.token, 51);
    assert_eq!(answer.outcome, Ok(("ahoy".to_string(), Vec::new())));
    assert!(h.ai().box_answer.is_none());

    let extra = h
        .driver
        .ai_apply_in_place(ChatId(7), "hello", "gone", 52)
        .unwrap();
    h.fail(extra, "AICOMPOSE_TONE_SLUG_INVALID");
    assert_eq!(
        h.ai().apply_in_place.clone().map(|answer| answer.outcome),
        Some(Err(AiBoxError::StyleInvalid(
            "AICOMPOSE_TONE_SLUG_INVALID".into()
        )))
    );
}

#[test]
fn saving_too_many_styles_shows_the_limit_toast() {
    let mut h = Harness::new();
    h.option("added_text_composition_style_count_max", 5);
    let extra = h.driver.add_text_composition_style("shared").unwrap();
    h.fail(extra, "TONES_SAVED_TOO_MANY");
    assert_eq!(
        h.ai().style_notice,
        Some(StyleNotice::Limit(StyleLimitNotice::Promo {
            count: 5,
            premium_count: 20
        }))
    );
    assert!(h.ai().op_names.is_empty());

    let extra = h
        .driver
        .create_text_composition_style("Pirate", 0, "arr", false)
        .unwrap();
    h.fail(extra, "TONES_SAVED_TOO_MANY");
    assert_eq!(
        h.ai().style_edit.clone().map(|answer| answer.outcome),
        Some(Err(StyleError::SavedTooMany))
    );
}

#[test]
fn style_lookup_fills_on_answer_or_error() {
    let mut h = Harness::new();
    let extra = h
        .driver
        .search_text_composition_style("pirate", 61)
        .unwrap();
    let lookup = h.ai().style_lookup.clone().expect("lookup");
    assert_eq!((lookup.token, lookup.name.as_str()), (61, "pirate"));
    assert!(lookup.outcome.is_none());
    h.answer(extra, style_json("pirate", true));
    assert!(matches!(
        h.ai().style_lookup.clone().and_then(|lookup| lookup.outcome),
        Some(Ok(style)) if style.name == "pirate"
    ));

    let extra = h.driver.search_text_composition_style("gone", 62).unwrap();
    h.fail(extra, "AICOMPOSE_TONE_SLUG_INVALID");
    assert_eq!(
        h.ai()
            .style_lookup
            .clone()
            .and_then(|lookup| lookup.outcome),
        Some(Err(StyleError::Invalid))
    );
}

#[test]
fn created_style_goes_to_the_front_and_answers_the_box() {
    let mut h = Harness::new();
    h.feed(json!({
        "@type": "updateTextCompositionStyles",
        "styles": [style_json("formal", false)]
    }));
    let extra = h
        .driver
        .create_text_composition_style("Pirate", 0, "arr", true)
        .unwrap();
    h.answer(extra, style_json("pirate", true));
    assert_eq!(h.ai().styles.names(), ["pirate", "formal"]);
    assert!(matches!(
        h.ai().style_edit.clone().map(|answer| answer.outcome),
        Some(Ok(style)) if style.name == "pirate"
    ));
    h.feed(json!({
        "@type": "updateTextCompositionStyles",
        "styles": [style_json("formal", false), style_json("pirate", true)]
    }));
    assert_eq!(h.ai().styles.names(), ["pirate", "formal"]);
}

#[test]
fn add_remove_and_delete_answers() {
    let mut h = Harness::new();
    h.feed(json!({
        "@type": "updateTextCompositionStyles",
        "styles": [style_json("formal", false), style_json("shared", true)]
    }));
    let extra = h.driver.add_text_composition_style("shared").unwrap();
    h.answer(extra, json!({"@type": "ok"}));
    assert_eq!(
        h.ai().style_notice,
        Some(StyleNotice::Added("shared".into()))
    );
    assert_eq!(h.ai().styles.names(), ["shared", "formal"]);

    let extra = h.driver.remove_text_composition_style("shared").unwrap();
    h.answer(extra, json!({"@type": "ok"}));
    assert_eq!(h.ai().style_notice, Some(StyleNotice::Removed));
    assert_eq!(h.ai().styles.names(), ["formal"]);

    let extra = h.driver.delete_text_composition_style("mine").unwrap();
    h.answer(extra, json!({"@type": "ok"}));
    assert_eq!(h.ai().style_notice, Some(StyleNotice::Deleted));

    h.driver.session.messages.ai_compose.style_notice = None;
    let extra = h.driver.delete_text_composition_style("mine").unwrap();
    h.fail(extra, "SOMETHING_ELSE");
    assert_eq!(h.ai().style_notice, None, "a failed delete is silent");
    assert!(h.ai().op_names.is_empty());
}

#[test]
fn style_example_answers_its_token() {
    let mut h = Harness::new();
    let extra = h
        .driver
        .get_text_composition_style_example("pirate", 0, 71)
        .unwrap();
    h.answer(
        extra,
        json!({
            "@type": "textCompositionStyleExample",
            "source_text": {"@type": "formattedText", "text": "Hello", "entities": []},
            "result_text": {"@type": "formattedText", "text": "Ahoy", "entities": []}
        }),
    );
    let example = h.ai().example.clone().expect("example");
    assert_eq!((example.token, example.style_name.as_str()), (71, "pirate"));
    assert!(matches!(example.outcome, Some(Ok(e)) if e.result_text == "Ahoy"));
}
