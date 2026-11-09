//! Connect-driver tests: message widgets' queries and public data polls
//! (recorded JSON, nothing live).
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{ChatId, MessageId};
use crate::message_widget::{QueryFailure, QueryOutcome, WidgetAnswer, WidgetKey};
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

type Recorder = Arc<RecordingSender>;

const KEY: WidgetKey = WidgetKey {
    chat_id: ChatId(-100),
    message_id: MessageId(7 << 20),
};

fn driver_parts() -> (ConnectDriver<Recorder>, Recorder, Arc<dyn DiagnosticSink>, AtomicU64) {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let driver = ready_driver(&recorder, prepared, &sink, &seq);
    (driver, recorder, sink, seq)
}

fn feed(driver: &mut ConnectDriver<Recorder>, seq: &AtomicU64, sink: &Arc<dyn DiagnosticSink>, value: serde_json::Value) {
    driver
        .ingest(copy_and_parse(&value.to_string(), seq, sink).unwrap())
        .unwrap();
}

#[test]
fn a_query_goes_out_as_plain_data_and_its_answer_comes_back_for_its_token() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    driver.send_widget_query(KEY, 3, b"vote:1").unwrap();
    let sent = sent_request(&recorder, "getWidgetQueryAnswer");
    assert_eq!(sent["chat_id"], -100);
    assert_eq!(sent["message_id"], 7 << 20);
    assert_eq!(sent["data"], "dm90ZTox");
    assert_eq!(
        sent["query_type"],
        json!({"@type": "widgetQueryTypeData", "with_wallet": false}),
        "never a wallet query"
    );
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type": "widgetQueryAnswer", "@extra": sent["@extra"], "data": "{\"votes\":4}"}),
    );
    assert_eq!(
        driver.take_widget_answers(),
        vec![WidgetAnswer::Query {
            key: KEY,
            token: 3,
            outcome: QueryOutcome::Answered("{\"votes\":4}".into()),
        }]
    );
    assert!(driver.take_widget_answers().is_empty(), "taken once");
}

#[test]
fn a_late_bot_is_a_timeout_and_a_flood_wait_is_rate_limited() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    for (token, code, message, failure) in [
        (1, 502, "Bot did not answer", QueryFailure::Timeout),
        (2, 429, "Too Many Requests: retry after 8", QueryFailure::RateLimited),
        (3, 400, "WIDGET_INVALID", QueryFailure::Failed),
    ] {
        driver.send_widget_query(KEY, token, b"x").unwrap();
        let sent = sent_request(&recorder, "getWidgetQueryAnswer");
        feed(
            &mut driver,
            &seq,
            &sink,
            json!({"@type": "error", "@extra": sent["@extra"], "code": code, "message": message}),
        );
        assert_eq!(
            driver.take_widget_answers(),
            vec![WidgetAnswer::Query {
                key: KEY,
                token,
                outcome: QueryOutcome::Failed(failure),
            }],
            "error {code}"
        );
    }
}

#[test]
fn an_empty_or_oversized_query_is_refused_before_tdlib() {
    let (mut driver, recorder, _sink, _seq) = driver_parts();
    assert!(driver.send_widget_query(KEY, 1, b"").is_err());
    assert!(driver.send_widget_query(KEY, 1, &[b'x'; 65]).is_err());
    let pending = WidgetKey {
        message_id: MessageId(-5),
        ..KEY
    };
    assert!(driver.send_widget_query(pending, 1, b"x").is_err());
    assert!(
        recorder
            .snapshot()
            .iter()
            .all(|json| !json.contains("getWidgetQueryAnswer"))
    );
}

#[test]
fn one_public_data_poll_at_a_time_and_its_period_is_held_to_bounds() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    assert!(driver.poll_widget_public_data(KEY).unwrap().is_some());
    assert!(driver.poll_widget_public_data(KEY).unwrap().is_none());
    let sent = sent_request(&recorder, "getWidgetPublicData");
    assert_eq!(sent["message_id"], 7 << 20);
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type": "widgetPublicData", "@extra": sent["@extra"], "data": "{\"n\":2}", "poll_period": 1}),
    );
    assert_eq!(
        driver.take_widget_answers(),
        vec![WidgetAnswer::PublicData {
            key: KEY,
            data: Some(("{\"n\":2}".into(), Duration::from_secs(5))),
        }]
    );
    assert!(driver.poll_widget_public_data(KEY).unwrap().is_some(), "answered: may poll again");
    let again = sent_request(&recorder, "getWidgetPublicData");
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type": "error", "@extra": again["@extra"], "code": 400, "message": "MESSAGE_ID_INVALID"}),
    );
    assert_eq!(
        driver.take_widget_answers(),
        vec![WidgetAnswer::PublicData { key: KEY, data: None }]
    );
}
