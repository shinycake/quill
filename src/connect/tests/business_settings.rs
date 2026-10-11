//! Connect-driver tests: Telegram Business settings (recorded JSON,
//! nothing live; no real account setting is changed).
use super::super::*;
use super::*;
use crate::business_settings::{
    AwaySchedule, AwaySettings, BusinessRecipients, BusinessSettingKind, GreetingSettings,
};
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
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
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let driver = ready_driver(&recorder, prepared, &sink, &seq);
    (driver, recorder, sink, seq)
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

fn count_sent(recorder: &Recorder, type_name: &str) -> usize {
    recorder
        .snapshot()
        .iter()
        .filter(|json| json.contains(&format!("\"@type\":\"{type_name}\"")))
        .count()
}

#[test]
fn time_zones_are_loaded_once() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    driver.ensure_time_zones().unwrap();
    driver.ensure_time_zones().unwrap();
    assert_eq!(count_sent(&recorder, "getTimeZones"), 1);
    let extra = sent_request(&recorder, "getTimeZones")["@extra"]
        .as_str()
        .unwrap()
        .to_string();
    feed(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"timeZones","@extra":"{extra}","time_zones":[{{"@type":"timeZone","id":"Europe/Paris","name":"Paris","utc_time_offset":7200}},{{"@type":"timeZone","id":"UTC","name":"UTC","utc_time_offset":0}}]}}"#
        ),
    );
    let zones = driver.session.settings.business.time_zones.clone().unwrap();
    assert_eq!(zones[0].id, "UTC");
    assert_eq!(zones[1].full_name(), "(UTC+2:00) Paris");
    driver.ensure_time_zones().unwrap();
    assert_eq!(count_sent(&recorder, "getTimeZones"), 1);
}

#[test]
fn a_greeting_save_is_tracked_until_ok() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    let settings = GreetingSettings {
        shortcut_id: 2,
        recipients: BusinessRecipients::everyone(),
        inactivity_days: 7,
    };
    driver.save_business_greeting(Some(&settings)).unwrap();
    assert_eq!(
        driver.session.settings.business.saving,
        vec![BusinessSettingKind::Greeting]
    );
    let request = sent_request(&recorder, "setBusinessGreetingMessageSettings");
    assert_eq!(request["greeting_message_settings"]["shortcut_id"], 2);
    let extra = request["@extra"].as_str().unwrap().to_string();
    feed(
        &mut driver,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{extra}"}}"#),
    );
    assert!(driver.session.settings.business.saving.is_empty());
    assert!(driver.session.settings.business.error.is_none());
}

#[test]
fn a_refused_save_says_so() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    driver.save_business_location(None).unwrap();
    let extra = sent_request(&recorder, "setBusinessLocation")["@extra"]
        .as_str()
        .unwrap()
        .to_string();
    feed(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{extra}","code":400,"message":"PREMIUM_ACCOUNT_REQUIRED"}}"#
        ),
    );
    let state = &driver.session.settings.business;
    assert!(state.saving.is_empty());
    assert_eq!(
        state.error.as_deref(),
        Some("Couldn't save the location: invalid request")
    );
}

#[test]
fn invalid_settings_are_not_sent() {
    let (mut driver, recorder, _sink, _seq) = driver_parts();
    let greeting = GreetingSettings {
        shortcut_id: 2,
        recipients: BusinessRecipients::everyone(),
        inactivity_days: 5,
    };
    assert!(driver.save_business_greeting(Some(&greeting)).is_err());
    let away = AwaySettings {
        shortcut_id: 3,
        recipients: BusinessRecipients::everyone(),
        schedule: AwaySchedule::Custom { start: 10, end: 5 },
        offline_only: false,
    };
    assert!(driver.save_business_away(Some(&away)).is_err());
    assert_eq!(
        count_sent(&recorder, "setBusinessGreetingMessageSettings"),
        0
    );
    assert_eq!(count_sent(&recorder, "setBusinessAwayMessageSettings"), 0);
    assert!(driver.session.settings.business.saving.is_empty());
}

#[test]
fn own_business_info_carries_greeting_and_away() {
    let (mut driver, _recorder, sink, seq) = driver_parts();
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateUserFullInfo","user_id":77,"user_full_info":{"@type":"userFullInfo","bio":{"@type":"formattedText","text":"","entities":[]},"business_info":{"@type":"businessInfo","location":null,"opening_hours":null,"local_opening_hours":null,"next_open_in":0,"next_close_in":0,"greeting_message_settings":{"@type":"businessGreetingMessageSettings","shortcut_id":5,"recipients":{"@type":"businessRecipients","chat_ids":[],"excluded_chat_ids":[],"select_existing_chats":false,"select_new_chats":false,"select_contacts":false,"select_non_contacts":false,"exclude_selected":true},"inactivity_days":28},"away_message_settings":{"@type":"businessAwayMessageSettings","shortcut_id":6,"recipients":{"@type":"businessRecipients","chat_ids":[],"excluded_chat_ids":[],"select_existing_chats":false,"select_new_chats":false,"select_contacts":true,"select_non_contacts":false,"exclude_selected":false},"schedule":{"@type":"businessAwayMessageScheduleOutsideOfOpeningHours"},"offline_only":false},"start_page":null}}}"#,
    );
    let business = driver
        .session
        .user_full_info(77)
        .and_then(|info| info.extras.business.clone())
        .unwrap();
    assert_eq!(business.greeting.unwrap().inactivity_days, 28);
    let away = business.away.unwrap();
    assert_eq!(away.schedule, AwaySchedule::OutsideOpeningHours);
    assert!(away.recipients.included.types.contacts);
    assert!(business.location.is_none());
}
