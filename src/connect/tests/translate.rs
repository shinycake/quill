//! Connect-driver tests: translation (`translateMessageText`,
//! `translateText`, `updateChatIsTranslatable`) against recorded TDLib JSON.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::{AccountKey, ChatId, MessageId};
use crate::platform::MemorySecretStore;
use crate::state::{Session, Translation};
use crate::telegram::client::copy_and_parse;
use crate::text::TextEntityKind;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

type Driver = ConnectDriver<Arc<RecordingSender>>;

struct Harness {
    dir: std::path::PathBuf,
    driver: Driver,
    recorder: Arc<RecordingSender>,
    sink: Arc<dyn DiagnosticSink>,
    seq: AtomicU64,
}

impl Harness {
    fn new() -> Self {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), dyn_sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &dyn_sink);
        Self {
            dir,
            driver,
            recorder,
            sink: dyn_sink,
            seq,
        }
    }

    fn ingest(&mut self, json: &str) {
        let owned = copy_and_parse(json, &self.seq, &self.sink).expect("parse");
        self.driver.ingest(owned).expect("ingest");
    }

    fn sent_count(&self) -> usize {
        self.recorder.snapshot().len()
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn translate_message_sends_the_request_and_keeps_the_entities() {
    let mut h = Harness::new();
    h.driver
        .translate_message(ChatId(7), MessageId(42), "en")
        .expect("translate request");
    let v = sent_request(&h.recorder, "translateMessageText");
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["message_id"], 42);
    assert_eq!(v["to_language_code"], "en");
    assert_eq!(v["tone"], "");
    assert_eq!(
        h.driver
            .session
            .message_translation(ChatId(7), MessageId(42), "en"),
        Some(&Translation::Pending)
    );

    // Asking again while it is pending sends nothing new.
    let before = h.sent_count();
    h.driver
        .translate_message(ChatId(7), MessageId(42), "en")
        .expect("duplicate is a no-op");
    assert_eq!(h.sent_count(), before);

    let json = format!(
        r#"{{"@type":"formattedText","text":"Hello bold world","entities":[{{"@type":"textEntity","offset":6,"length":4,"type":{{"@type":"textEntityTypeBold"}}}}],"@extra":"{id}"}}"#,
        id = v["@extra"].as_str().unwrap(),
    );
    h.ingest(&json);
    match h
        .driver
        .session
        .message_translation(ChatId(7), MessageId(42), "en")
    {
        Some(Translation::Done { text, entities }) => {
            assert_eq!(text, "Hello bold world");
            assert_eq!(entities.len(), 1);
            assert_eq!(entities[0].kind, TextEntityKind::Bold);
            assert_eq!((entities[0].utf8_start, entities[0].utf8_end), (6, 10));
        }
        other => panic!("expected a finished translation, got {other:?}"),
    }
    // A translation into another language is a separate request.
    h.driver
        .translate_message(ChatId(7), MessageId(42), "de")
        .expect("another language");
    assert_eq!(
        h.driver
            .session
            .message_translation(ChatId(7), MessageId(42), "de"),
        Some(&Translation::Pending)
    );
}

#[test]
fn a_failed_translation_is_recorded_and_can_be_retried() {
    let mut h = Harness::new();
    h.driver
        .translate_message(ChatId(7), MessageId(5), "fr")
        .expect("request");
    let v = sent_request(&h.recorder, "translateMessageText");
    let json = format!(
        r#"{{"@type":"error","code":400,"message":"MESSAGE_ID_INVALID","@extra":"{id}"}}"#,
        id = v["@extra"].as_str().unwrap(),
    );
    h.ingest(&json);
    assert!(matches!(
        h.driver
            .session
            .message_translation(ChatId(7), MessageId(5), "fr"),
        Some(Translation::Failed(_))
    ));
    let before = h.sent_count();
    h.driver
        .translate_message(ChatId(7), MessageId(5), "fr")
        .expect("retry");
    assert_eq!(h.sent_count(), before + 1);
}

#[test]
fn translate_selection_round_trips_a_text_job() {
    let mut h = Harness::new();
    let job = h
        .driver
        .translate_selection("Привет, мир", "en")
        .expect("request");
    let v = sent_request(&h.recorder, "translateText");
    assert_eq!(v["text"]["text"], "Привет, мир");
    assert_eq!(v["to_language_code"], "en");
    assert_eq!(
        h.driver.session.text_translation(job),
        Some(&Translation::Pending)
    );
    let json = format!(
        r#"{{"@type":"formattedText","text":"Hello, world","entities":[],"@extra":"{id}"}}"#,
        id = v["@extra"].as_str().unwrap(),
    );
    h.ingest(&json);
    assert_eq!(
        h.driver.session.text_translation(job),
        Some(&Translation::Done {
            text: "Hello, world".into(),
            entities: Vec::new(),
        })
    );
    assert!(h.driver.translate_selection("   ", "en").is_err());
}

#[test]
fn chat_translatable_flag_follows_the_update() {
    let mut h = Harness::new();
    assert!(!h.driver.session.chat_is_translatable(ChatId(7)));
    h.ingest(r#"{"@type":"updateChatIsTranslatable","chat_id":7,"is_translatable":true}"#);
    assert!(h.driver.session.chat_is_translatable(ChatId(7)));
    h.ingest(r#"{"@type":"updateChatIsTranslatable","chat_id":7,"is_translatable":false}"#);
    assert!(!h.driver.session.chat_is_translatable(ChatId(7)));
}

#[test]
fn chat_translated_to_toggles_and_bumps_the_revision() {
    let mut h = Harness::new();
    let before = h.driver.session.translate.revision;
    h.driver
        .session
        .set_chat_translated_to(ChatId(7), Some("en"));
    assert_eq!(h.driver.session.chat_translated_to(ChatId(7)), Some("en"));
    assert!(h.driver.session.translate.revision > before);
    let after = h.driver.session.translate.revision;
    h.driver
        .session
        .set_chat_translated_to(ChatId(7), Some("en"));
    assert_eq!(
        h.driver.session.translate.revision,
        after,
        "no change, no bump"
    );
    h.driver.session.set_chat_translated_to(ChatId(7), None);
    assert_eq!(h.driver.session.chat_translated_to(ChatId(7)), None);
}
