use super::*;
use serde_json::{Value, json};

fn parsed(json: &str) -> Value {
    serde_json::from_str(json).unwrap()
}

const GREETING: &str = r#"{"@type":"businessGreetingMessageSettings","shortcut_id":12,"recipients":{"@type":"businessRecipients","chat_ids":[101,102],"excluded_chat_ids":[],"select_existing_chats":false,"select_new_chats":true,"select_contacts":false,"select_non_contacts":true,"exclude_selected":true},"inactivity_days":14}"#;

const AWAY: &str = r#"{"@type":"businessAwayMessageSettings","shortcut_id":13,"recipients":{"@type":"businessRecipients","chat_ids":[7],"excluded_chat_ids":[],"select_existing_chats":true,"select_new_chats":false,"select_contacts":false,"select_non_contacts":false,"exclude_selected":false},"schedule":{"@type":"businessAwayMessageScheduleCustom","start_date":1700000000,"end_date":1700086400},"offline_only":true}"#;

#[test]
fn parses_greeting_with_excluded_recipients() {
    let greeting = parse_greeting(Some(&parsed(GREETING))).unwrap();
    assert_eq!(greeting.shortcut_id, 12);
    assert_eq!(greeting.inactivity_days, 14);
    let r = &greeting.recipients;
    assert!(r.all_but_excluded);
    assert_eq!(r.excluded.chat_ids, vec![101, 102]);
    assert!(r.excluded.types.new_chats && r.excluded.types.non_contacts);
    assert!(!r.excluded.types.contacts);
    assert!(r.included.is_empty());
}

#[test]
fn parses_away_with_custom_schedule_and_included_chats() {
    let away = parse_away(Some(&parsed(AWAY))).unwrap();
    assert_eq!(away.shortcut_id, 13);
    assert!(away.offline_only);
    assert_eq!(
        away.schedule,
        AwaySchedule::Custom {
            start: 1_700_000_000,
            end: 1_700_086_400
        }
    );
    assert!(!away.recipients.all_but_excluded);
    assert_eq!(away.recipients.included.chat_ids, vec![7]);
    assert!(away.recipients.included.types.existing_chats);
}

#[test]
fn null_settings_are_off() {
    assert!(parse_greeting(Some(&Value::Null)).is_none());
    assert!(parse_away(None).is_none());
}

#[test]
fn recipients_round_trip_through_json() {
    let away = parse_away(Some(&parsed(AWAY))).unwrap();
    let json = away.recipients.to_json();
    assert_eq!(BusinessRecipients::from_value(&json), away.recipients);
    assert_eq!(json["excluded_chat_ids"], json!([]));
    assert_eq!(json["@type"], "businessRecipients");
}

#[test]
fn only_the_active_side_is_sent() {
    let mut r = BusinessRecipients::everyone();
    r.included.chat_ids = vec![5];
    r.excluded.chat_ids = vec![9];
    let json = r.to_json();
    assert_eq!(json["chat_ids"], json!([9]));
    assert_eq!(json["exclude_selected"], true);
}

#[test]
fn make_valid_falls_back_to_all_except() {
    let r = BusinessRecipients::default().make_valid();
    assert!(r.all_but_excluded);
    let mut selected = BusinessRecipients::default();
    selected.included.chat_ids = vec![1];
    assert!(!selected.make_valid().all_but_excluded);
}

#[test]
fn picker_saves_follow_tdesktop() {
    let mut r = BusinessRecipients::everyone();
    r.excluded.types.contacts = true;
    // Including chats drops excluded categories and selects "only".
    r.set_included(BusinessChats {
        types: ChatTypes::default(),
        chat_ids: vec![4],
    });
    assert!(!r.all_but_excluded);
    assert!(r.excluded.types.is_empty());
    // A non-empty exclusion drops the included list.
    r.set_excluded(BusinessChats {
        types: ChatTypes::default(),
        chat_ids: vec![8],
    });
    assert!(r.included.is_empty());
    // An empty inclusion goes back to "all except".
    r.set_included(BusinessChats::default());
    assert!(r.all_but_excluded);
}

#[test]
fn chat_toggle_stops_at_the_limit() {
    let mut chats = BusinessChats::default();
    for id in 0..RECIPIENT_CHATS_MAX as i64 {
        assert!(chats.toggle_chat(id));
    }
    assert!(!chats.toggle_chat(1000));
    assert!(chats.toggle_chat(3));
    assert_eq!(chats.chat_ids.len(), RECIPIENT_CHATS_MAX - 1);
}

#[test]
fn greeting_request_json() {
    let mut recipients = BusinessRecipients::everyone();
    recipients.excluded.types.contacts = true;
    let settings = GreetingSettings {
        shortcut_id: 3,
        recipients,
        inactivity_days: 21,
    };
    let body: Value = serde_json::from_str(&set_greeting_request("9", Some(&settings))).unwrap();
    assert_eq!(body["@type"], "setBusinessGreetingMessageSettings");
    assert_eq!(body["@extra"], "9");
    let s = &body["greeting_message_settings"];
    assert_eq!(s["@type"], "businessGreetingMessageSettings");
    assert_eq!(s["shortcut_id"], 3);
    assert_eq!(s["inactivity_days"], 21);
    assert_eq!(s["recipients"]["select_contacts"], true);
    assert_eq!(s["recipients"]["exclude_selected"], true);
    let off: Value = serde_json::from_str(&set_greeting_request("9", None)).unwrap();
    assert!(off["greeting_message_settings"].is_null());
}

#[test]
fn away_request_json_for_each_schedule() {
    let base = AwaySettings {
        shortcut_id: 4,
        recipients: BusinessRecipients::everyone(),
        schedule: AwaySchedule::Always,
        offline_only: false,
    };
    let body = |schedule| -> Value {
        let s = AwaySettings {
            schedule,
            ..base.clone()
        };
        serde_json::from_str(&set_away_request("1", Some(&s))).unwrap()
    };
    assert_eq!(
        body(AwaySchedule::Always)["away_message_settings"]["schedule"]["@type"],
        "businessAwayMessageScheduleAlways"
    );
    assert_eq!(
        body(AwaySchedule::OutsideOpeningHours)["away_message_settings"]["schedule"]["@type"],
        "businessAwayMessageScheduleOutsideOfOpeningHours"
    );
    let custom = body(AwaySchedule::Custom {
        start: 1_800_000_000,
        end: 1_800_003_600,
    });
    let schedule = &custom["away_message_settings"]["schedule"];
    assert_eq!(schedule["@type"], "businessAwayMessageScheduleCustom");
    assert_eq!(schedule["start_date"], 1_800_000_000);
    assert_eq!(schedule["end_date"], 1_800_003_600);
    assert_eq!(custom["away_message_settings"]["offline_only"], false);
    let off: Value = serde_json::from_str(&set_away_request("1", None)).unwrap();
    assert!(off["away_message_settings"].is_null());
}

#[test]
fn opening_hours_and_location_requests() {
    let hours = OpeningHours::new("Europe/Berlin".into(), vec![(540, 1020), (1980, 2460)]);
    let body: Value = serde_json::from_str(&set_opening_hours_request("2", Some(&hours))).unwrap();
    assert_eq!(body["@type"], "setBusinessOpeningHours");
    assert_eq!(body["opening_hours"]["time_zone_id"], "Europe/Berlin");
    assert_eq!(
        body["opening_hours"]["opening_hours"][1]["start_minute"],
        1980
    );
    assert_eq!(
        body["opening_hours"]["opening_hours"][1]["end_minute"],
        2460
    );
    let off: Value = serde_json::from_str(&set_opening_hours_request("2", None)).unwrap();
    assert!(off["opening_hours"].is_null());

    let location = location_to_save("  1 Main St  ", Some((32_085_300, 34_781_800))).unwrap();
    assert_eq!(location.address, "1 Main St");
    let body: Value = serde_json::from_str(&set_location_request("3", Some(&location))).unwrap();
    assert_eq!(body["location"]["address"], "1 Main St");
    assert_eq!(body["location"]["location"]["latitude"], 32.0853);
    let no_point = location_to_save("Shop", None).unwrap();
    let body: Value = serde_json::from_str(&set_location_request("3", Some(&no_point))).unwrap();
    assert!(body["location"]["location"].is_null());
    assert!(location_to_save("   ", None).is_none());
    let off: Value = serde_json::from_str(&set_location_request("3", None)).unwrap();
    assert!(off["location"].is_null());
}

#[test]
fn custom_interval_defaults_and_validation() {
    assert!(bad_custom_interval(0, 10));
    assert!(bad_custom_interval(1_800_000_000, 1_800_000_000));
    assert!(!bad_custom_interval(1_800_000_000, 1_800_000_001));
    let now = 1_800_000_000;
    assert_eq!(initial_custom_interval(None, now), (now, now + 86_400));
    assert_eq!(
        initial_custom_interval(Some(AwaySchedule::Custom { start: 5, end: 6 }), now),
        (now, now + 86_400)
    );
    let saved = AwaySchedule::Custom {
        start: now - 10,
        end: now + 10,
    };
    assert_eq!(
        initial_custom_interval(Some(saved), now),
        (now - 10, now + 10)
    );
}

#[test]
fn enabling_respects_the_quick_reply_limit() {
    assert!(can_enable(3, 100, false));
    assert!(!can_enable(100, 100, false));
    assert!(can_enable(100, 100, true));
}

#[test]
fn error_lines() {
    assert_eq!(
        save_error_line(BusinessSettingKind::Location, "invalid request"),
        "Couldn't save the location: invalid request"
    );
}

#[test]
fn time_zones_parse_sort_and_match() {
    let value = parsed(
        r#"{"@type":"timeZones","time_zones":[{"@type":"timeZone","id":"Asia/Jerusalem","name":"Jerusalem","utc_time_offset":7200},{"@type":"timeZone","id":"America/New_York","name":"New York","utc_time_offset":-14400},{"@type":"timeZone","id":"Asia/Kolkata","name":"Kolkata","utc_time_offset":19800}]}"#,
    );
    let zones = parse_time_zones(&value);
    assert_eq!(zones[0].id, "America/New_York");
    assert_eq!(zones[0].full_name(), "(UTC-4:00) New York");
    assert_eq!(zones[2].full_name(), "(UTC+5:30) Kolkata");
    assert_eq!(
        closest_time_zone(&zones, 3 * 3600).unwrap().id,
        "Asia/Jerusalem"
    );
    assert_eq!(
        closest_time_zone(&zones, -5 * 3600).unwrap().id,
        "America/New_York"
    );
    assert!(closest_time_zone(&[], 0).is_none());
}

#[test]
fn days_wording() {
    assert_eq!(days_label(7), "7 days");
    assert_eq!(days_label(1), "1 day");
}
