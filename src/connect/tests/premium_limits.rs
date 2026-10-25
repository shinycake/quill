use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::platform::MemorySecretStore;
use crate::premium_limits::{LimitPair, PremiumLimitKind};
use crate::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

type Recorder = Arc<RecordingSender>;

fn driver_parts() -> (
    ConnectDriver<Recorder>,
    Recorder,
    Arc<dyn DiagnosticSink>,
    AtomicU64,
) {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink;
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    (driver, recorder, dyn_sink, seq)
}

fn feed(
    driver: &mut ConnectDriver<Recorder>,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
    json: &str,
) {
    driver
        .ingest(copy_and_parse(json, seq, sink).unwrap())
        .unwrap();
}

fn limit_requests(recorder: &Recorder) -> usize {
    recorder
        .snapshot()
        .iter()
        .filter(|json| json.contains("\"getPremiumLimit\""))
        .count()
}

#[test]
fn the_saved_pins_limit_is_asked_once_and_its_answer_is_used() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    assert_eq!(
        driver
            .session
            .premium_limit_pair(PremiumLimitKind::SavedPins),
        PremiumLimitKind::SavedPins.fallback()
    );
    driver
        .ensure_premium_limit(PremiumLimitKind::SavedPins)
        .unwrap()
        .expect("a request");
    let request = sent_request(&recorder, "getPremiumLimit");
    assert_eq!(
        request["limit_type"]["@type"],
        "premiumLimitTypePinnedSavedMessagesTopicCount"
    );
    assert_eq!(
        driver.ensure_premium_limit(PremiumLimitKind::SavedPins),
        Ok(None)
    );
    let extra = request["@extra"].as_str().unwrap();
    feed(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"premiumLimit","@extra":"{extra}","type":{{"@type":"premiumLimitTypePinnedSavedMessagesTopicCount"}},"default_value":6,"premium_value":120}}"#
        ),
    );
    assert_eq!(
        driver
            .session
            .premium_limit_pair(PremiumLimitKind::SavedPins),
        LimitPair {
            default_value: 6,
            premium_value: 120
        }
    );
    assert_eq!(
        driver
            .session
            .premium_limit_in_force(PremiumLimitKind::SavedPins),
        6
    );
    driver.session.payments.premium_option = Some(true);
    assert_eq!(
        driver
            .session
            .premium_limit_in_force(PremiumLimitKind::SavedPins),
        120
    );
    assert_eq!(limit_requests(&recorder), 1);
}

#[test]
fn file_size_and_limits_known_from_premium_features_are_not_asked() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    assert_eq!(
        driver.ensure_premium_limit(PremiumLimitKind::FileSize),
        Ok(None)
    );
    driver.maybe_fetch_premium().unwrap();
    let features = sent_request(&recorder, "getPremiumFeatures");
    let extra = features["@extra"].as_str().unwrap();
    feed(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"premiumFeatures","@extra":"{extra}","features":[],"limits":[{{"@type":"premiumLimit","type":{{"@type":"premiumLimitTypeSimilarChatCount"}},"default_value":12,"premium_value":90}}],"payment_link":null}}"#
        ),
    );
    assert_eq!(
        driver.ensure_premium_limit(PremiumLimitKind::SimilarChats),
        Ok(None)
    );
    assert_eq!(limit_requests(&recorder), 0);
    assert_eq!(
        driver
            .session
            .premium_limit_pair(PremiumLimitKind::SimilarChats)
            .premium_value,
        90
    );
}

#[test]
fn the_limit_options_set_the_limit_in_force() {
    let (mut driver, _recorder, sink, seq) = driver_parts();
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateOption","name":"favorite_stickers_limit","value":{"@type":"optionValueInteger","value":"7"}}"#,
    );
    assert_eq!(
        driver
            .session
            .premium_limit_in_force(PremiumLimitKind::FavoriteStickers),
        7
    );
}
