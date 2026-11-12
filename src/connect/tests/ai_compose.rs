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

    fn ai_mut(&mut self) -> &mut crate::state::AiComposeState {
        &mut self.driver.session.messages.ai_compose
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
        .ai_box_request(ChatId(7), &compose("Hallo", "en", "formal", true))
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
            .ai_box_request(ChatId(7), &compose("  ", "en", "", false)),
    );
    assert_invalid(h.driver.ai_apply_in_place(ChatId(7), "text", ""));
    h.feed(
        json!({"@type":"updateNewChat","chat":{"id":31,"title":"Secret",
        "type":{"@type":"chatTypeSecret","secret_chat_id":31,"user_id":7},"unread_count":0}}),
    );
    assert_invalid(
        h.driver
            .ai_box_request(ChatId(31), &compose("hi", "en", "", false)),
    );
    assert_invalid(h.driver.ai_apply_in_place(ChatId(31), "hi", "pirate"));
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

    h.driver.search_text_composition_style("pirate").unwrap();
    assert_eq!(h.sent("searchTextCompositionStyle")["name"], "pirate");
    h.driver
        .get_text_composition_style_example("pirate", 2)
        .unwrap();
    let v = h.sent("getTextCompositionStyleExample");
    assert_eq!(v["name"], "pirate");
    assert_eq!(v["example_number"], 2);

    let extra = h
        .driver
        .ai_apply_in_place(ChatId(7), "selected text", "pirate")
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
    assert_invalid(h.driver.get_text_composition_style_example("pirate", -1));
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
    assert_invalid(h.driver.get_text_composition_style_example("pirate", 3));
    assert!(
        h.driver
            .get_text_composition_style_example("pirate", 2)
            .is_ok()
    );
}

#[test]
fn style_list_update_populates_the_tabs() {
    let mut h = Harness::new();
    assert!(h.ai().styles.is_empty());
    h.feed(json!({
        "@type": "updateTextCompositionStyles",
        "styles": [style_json("formal", false), style_json("pirate", true)]
    }));
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
    let answer = h.ai_mut().box_answers.take(extra).expect("box answer");
    let result = answer.expect("fixed");
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
    assert!(h.driver.session.messages.ai_answers.is_empty());
}

#[test]
fn formatted_text_for_the_box_never_touches_the_draft() {
    let mut h = Harness::new();
    let extra = h
        .driver
        .ai_box_request(ChatId(7), &compose("Hallo", "en", "", false))
        .unwrap();
    h.answer(
        extra,
        json!({"@type": "formattedText", "text": "Hello", "entities": [
            {"@type": "textEntity", "offset": 0, "length": 5, "type": {"@type": "textEntityTypeBold"}}
        ]}),
    );
    let answer = h.ai_mut().box_answers.take(extra).expect("box answer");
    let result = answer.expect("composed");
    assert_eq!(result.text, "Hello");
    assert_eq!(result.entities.len(), 1);
    assert!(result.diff.is_none());
    assert!(h.driver.session.messages.ai_answers.is_empty());
    assert!(h.ai().apply_answers.is_empty());
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
            .ai_box_request(ChatId(7), &compose("Hallo", "en", "", false))
            .unwrap();
        h.fail(extra, message);
        let answer = h.ai_mut().box_answers.take(extra).expect("box answer");
        assert_eq!(answer, Err(expected), "{message}");
    }
    assert!(h.driver.session.messages.ai_error.is_empty());
}

#[test]
fn apply_in_place_answers_and_style_errors() {
    let mut h = Harness::new();
    let extra = h
        .driver
        .ai_apply_in_place(ChatId(7), "hello", "pirate")
        .unwrap();
    h.answer(
        extra,
        json!({"@type": "formattedText", "text": "ahoy", "entities": []}),
    );
    let answer = h.ai_mut().apply_answers.take(extra).expect("answer");
    assert_eq!(answer, Ok(("ahoy".to_string(), Vec::new())));
    assert!(h.ai().box_answers.is_empty());

    let extra = h
        .driver
        .ai_apply_in_place(ChatId(7), "hello", "gone")
        .unwrap();
    h.fail(extra, "AICOMPOSE_TONE_SLUG_INVALID");
    assert_eq!(
        h.ai_mut().apply_answers.take(extra),
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
        h.ai_mut().take_style_notices(),
        [StyleNotice::Limit(StyleLimitNotice::Promo {
            count: 5,
            premium_count: 20
        })]
    );
    assert!(h.ai().op_names.is_empty());

    let extra = h
        .driver
        .create_text_composition_style("Pirate", 0, "arr", false)
        .unwrap();
    h.fail(extra, "TONES_SAVED_TOO_MANY");
    assert_eq!(
        h.ai_mut().style_edits.take(extra),
        Some(Err(StyleError::SavedTooMany))
    );
}

#[test]
fn style_lookup_fills_on_answer_or_error() {
    let mut h = Harness::new();
    let extra = h.driver.search_text_composition_style("pirate").unwrap();
    let lookup = h.ai().style_lookup.clone().expect("lookup");
    assert_eq!(lookup.name.as_str(), "pirate");
    assert_eq!(lookup.outcome.request(), extra);
    assert!(!lookup.outcome.is_done());
    h.answer(extra, style_json("pirate", true));
    assert!(matches!(
        h.ai().style_lookup.clone().and_then(|lookup| lookup.outcome.done()),
        Some(Ok(style)) if style.name == "pirate"
    ));

    let extra = h.driver.search_text_composition_style("gone").unwrap();
    h.fail(extra, "AICOMPOSE_TONE_SLUG_INVALID");
    assert_eq!(
        h.ai()
            .style_lookup
            .clone()
            .and_then(|lookup| lookup.outcome.done()),
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
        h.ai_mut().style_edits.take(extra),
        Some(Ok(style)) if style.name == "pirate"
    ));
    h.feed(json!({
        "@type": "updateTextCompositionStyles",
        "styles": [style_json("formal", false), style_json("pirate", true)]
    }));
    assert_eq!(h.ai().styles.names(), ["pirate", "formal"]);
}

/// Save style X, press Back, open the editor for Y: the answers are filed
/// under their own requests, so Y's editor takes Y's and X's stays X's (two
/// answers in one drain overwrite nothing); X still joins the list.
#[test]
fn a_save_answer_names_the_request_it_answers() {
    let mut h = Harness::new();
    let x = h
        .driver
        .create_text_composition_style("X", 0, "arr", true)
        .unwrap();
    h.answer(x, style_json("x", true));
    assert!(h.ai().style_edits.has(x));
    assert_eq!(h.ai().styles.names(), ["x"]);

    let failed = h
        .driver
        .edit_text_composition_style("x", "X", 0, "arr", false)
        .unwrap();
    h.fail(failed, "TONES_SAVED_TOO_MANY");
    assert!(h.ai().style_edits.has(failed));
    assert!(
        h.ai().style_edits.has(x),
        "the second answer overwrote nothing"
    );

    let y = h
        .driver
        .create_text_composition_style("Y", 0, "arr", true)
        .unwrap();
    h.answer(y, style_json("y", true));
    assert!(matches!(
        h.ai_mut().style_edits.take(y),
        Some(Ok(style)) if style.name == "y"
    ));
    assert_eq!(
        h.ai_mut().style_edits.take(failed),
        Some(Err(StyleError::SavedTooMany))
    );
    assert!(matches!(
        h.ai_mut().style_edits.take(x),
        Some(Ok(style)) if style.name == "x"
    ));
    assert!(h.ai().style_edits.is_empty());
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
        h.ai_mut().take_style_notices(),
        [StyleNotice::Added {
            name: "shared".into(),
            request: extra
        }]
    );
    assert_eq!(h.ai().styles.names(), ["shared", "formal"]);

    let extra = h.driver.remove_text_composition_style("shared").unwrap();
    h.answer(extra, json!({"@type": "ok"}));
    assert_eq!(
        h.ai_mut().take_style_notices(),
        [StyleNotice::Removed { request: extra }]
    );
    assert_eq!(h.ai().styles.names(), ["formal"]);

    let extra = h.driver.delete_text_composition_style("mine").unwrap();
    h.answer(extra, json!({"@type": "ok"}));
    assert_eq!(
        h.ai_mut().take_style_notices(),
        [StyleNotice::Deleted { request: extra }]
    );
    let extra = h.driver.delete_text_composition_style("mine").unwrap();
    h.fail(extra, "SOMETHING_ELSE");
    assert!(
        h.ai_mut().take_style_notices().is_empty(),
        "a failed delete is silent"
    );
    assert_eq!(
        h.ai_mut().take_style_op_failures(),
        [extra],
        "but the preview waiting on it is told"
    );
    assert!(h.ai().op_names.is_empty());
}

#[test]
fn style_example_answers_its_request() {
    let mut h = Harness::new();
    let extra = h
        .driver
        .get_text_composition_style_example("pirate", 0)
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
    assert_eq!(
        (example.outcome.request(), example.style_name.as_str()),
        (extra, "pirate")
    );
    assert!(matches!(example.outcome.answer(), Some(Ok(e)) if e.result_text == "Ahoy"));
}

/// 08-008: a lookup answers only the request it runs as; an older lookup's
/// answer finds the slot running another and is dropped.
#[test]
fn an_older_lookups_answer_does_not_settle_the_newer_one() {
    let mut h = Harness::new();
    let older = h.driver.search_text_composition_style("pirate").unwrap();
    let newer = h.driver.search_text_composition_style("poet").unwrap();
    h.answer(older, style_json("pirate", true));
    let lookup = h.ai().style_lookup.clone().expect("lookup");
    assert_eq!(lookup.name, "poet");
    assert!(
        !lookup.outcome.is_done(),
        "still waiting for its own request"
    );
    assert_eq!(lookup.outcome.request(), newer);
    h.fail(older, "AICOMPOSE_TONE_SLUG_INVALID");
    assert!(
        !h.ai()
            .style_lookup
            .clone()
            .expect("lookup")
            .outcome
            .is_done()
    );
    h.answer(newer, style_json("poet", true));
    assert!(
        h.ai()
            .style_lookup
            .clone()
            .expect("lookup")
            .outcome
            .is_done()
    );
}

/// 08-009: two failed style operations in one drain both toast, and both
/// previews waiting on them are told.
#[test]
fn two_failed_style_operations_in_one_drain_are_both_kept() {
    let mut h = Harness::new();
    let add = h.driver.add_text_composition_style("shared").unwrap();
    let remove = h.driver.remove_text_composition_style("mine").unwrap();
    h.fail(add, "SOMETHING_ELSE");
    h.fail(remove, "AICOMPOSE_TONE_INVALID");
    let notices = h.ai_mut().take_style_notices();
    assert_eq!(notices.len(), 2, "{notices:?}");
    assert_eq!(h.ai_mut().take_style_op_failures(), [add, remove]);
    assert!(h.ai_mut().take_style_notices().is_empty());
}

/// Notices nobody drains stay bounded.
#[test]
fn undrained_style_notices_stay_bounded() {
    let mut h = Harness::new();
    for _ in 0..40 {
        let add = h.driver.add_text_composition_style("shared").unwrap();
        h.fail(add, "SOMETHING_ELSE");
    }
    assert!(h.ai_mut().take_style_notices().len() <= 8);
    assert!(h.ai_mut().take_style_op_failures().len() <= 8);
}
