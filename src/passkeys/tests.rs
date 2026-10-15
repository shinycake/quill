use super::*;
use crate::local_time::civil_at;

fn key(id: &str, name: &str) -> Passkey {
    Passkey {
        id: id.into(),
        name: name.into(),
        addition_date: 1_790_000_000,
        last_usage_date: 0,
        software_icon_custom_emoji_id: 0,
    }
}

fn known(list: Vec<Passkey>) -> PasskeysState {
    let mut state = PasskeysState {
        display_allowed: true,
        ..Default::default()
    };
    state.accept_list(list);
    state
}

#[test]
fn parses_the_list_and_skips_passkeys_without_an_id() {
    let value = serde_json::json!({"@type": "passkeys", "passkeys": [
        {"@type": "passkey", "id": "a1", "name": "iCloud Keychain", "addition_date": 1_790_000_000,
         "last_usage_date": 1_790_100_000, "software_icon_custom_emoji_id": "5368324170671202286"},
        {"@type": "passkey", "id": "", "name": "broken"},
        {"@type": "passkey", "id": "b2", "name": ""}
    ]});
    let Some(PasskeysPayload::List(list)) = parse_payload("passkeys", &value) else {
        panic!("not a list");
    };
    assert_eq!(list.len(), 2);
    let first = list.first().unwrap();
    assert_eq!(
        first.software_icon_custom_emoji_id,
        5_368_324_170_671_202_286
    );
    assert_eq!(first.last_usage_date, 1_790_100_000);
    assert_eq!(list.get(1).unwrap().title(), "Passkey");
    assert!(parse_payload("passkey", &value).is_none());
}

#[test]
fn requests_carry_the_extra_and_the_id() {
    let get: Value = serde_json::from_str(&get_login_passkeys(RequestId(4))).unwrap();
    assert_eq!(get["@type"], "getLoginPasskeys");
    assert_eq!(get["@extra"], RequestId(4).as_extra());
    let remove: Value = serde_json::from_str(&remove_login_passkey(RequestId(5), "a1")).unwrap();
    assert_eq!(remove["@type"], "removeLoginPasskey");
    assert_eq!(remove["passkey_id"], "a1");
}

#[test]
fn the_row_label_follows_tdesktop() {
    let loading = PasskeysState::default();
    assert_eq!(loading.row_label(), RowLabel::Loading);
    assert_eq!(loading.row_label().text(), "Loading...");
    assert_eq!(known(vec![]).row_label().text(), "Off");
    assert_eq!(
        known(vec![key("a", "YubiKey")]).row_label(),
        RowLabel::Name("YubiKey".into())
    );
    // A single unnamed passkey shows its (empty) name, as tdesktop does.
    assert_eq!(known(vec![key("a", "")]).row_label().text(), "");
    assert_eq!(
        known(vec![key("a", "x"), key("b", "y")]).row_label().text(),
        "2"
    );
}

#[test]
fn the_row_follows_webauthn_and_the_passkeys() {
    // WebAuthn: a USB key over CTAP2 on macOS and Linux, webauthn.dll on
    // Windows when the system has it.
    assert_eq!(
        webauthn_supported(),
        cfg!(any(target_os = "macos", target_os = "linux")) || crate::webauthn::system_dialog()
    );
    assert_eq!(
        crate::webauthn::system_dialog(),
        cfg!(windows) && webauthn_supported()
    );
    // Without passkeys the row shows only when one can be created.
    assert_eq!(known(vec![]).row_shown(), webauthn_supported());
    assert!(known(vec![key("a", "x")]).row_shown());
    // The server's `settings_display_passkeys` hides it either way.
    let mut hidden = known(vec![key("a", "x")]);
    hidden.display_allowed = false;
    assert!(!hidden.row_shown());
    assert_eq!(known(vec![]).can_register(), webauthn_supported());
}

#[test]
fn registering_stops_at_the_passkey_limit() {
    let mut state = known(vec![key("a", "x"), key("b", "y")]);
    assert_eq!(state.can_register(), webauthn_supported());
    state.max_passkeys = Some(2);
    assert!(!state.can_register());
    // Ten when the app config does not say (tdesktop's default).
    state.max_passkeys = None;
    state.list = (0..10).map(|i| key(&i.to_string(), "k")).collect();
    assert!(!state.can_register());
}

#[test]
fn parameters_start_the_ceremony_and_the_added_passkey_joins_the_list() {
    let mut state = known(vec![key("a", "x")]);
    let now = Instant::now();
    assert!(!state.accept_parameters("{}", now));
    assert!(state.ceremony.is_none());
    let json = r#"{"publicKey":{"rp":{"id":"telegram.org"},"challenge":"AQ","timeout":1000}}"#;
    assert!(state.accept_parameters(json, now));
    let options = state.ceremony.take().unwrap();
    assert_eq!(options.rp_id, "telegram.org");
    // The options stay fresh for their own timeout.
    let pending = state.registration.clone().unwrap();
    assert!(pending.fresh(now + Duration::from_millis(999)));
    assert!(!pending.fresh(now + Duration::from_millis(1000)));
    state.adding = true;
    state.accept_added(key("n", "YubiKey"));
    assert_eq!(state.list.len(), 2);
    assert_eq!(state.list.last().unwrap().id, "n");
    assert!(state.added && !state.adding && state.registration.is_none());
    let value = serde_json::json!({"@type": "passkey", "id": "n", "name": "YubiKey"});
    assert!(matches!(
        parse_payload("passkey", &value),
        Some(PasskeysPayload::Added(p)) if p.id == "n"
    ));
}

#[test]
fn add_login_passkey_sends_base64_bytes() {
    let registration = crate::webauthn::Registration {
        credential_id: vec![1],
        attestation_object: vec![0xA3, 0xFF],
        client_data_json: r#"{"type":"webauthn.create"}"#.into(),
    };
    let json: serde_json::Value =
        serde_json::from_str(&add_login_passkey(RequestId(7), &registration)).unwrap();
    assert_eq!(json["@type"], "addLoginPasskey");
    assert_eq!(json["attestation_object"], "o/8=");
    assert_eq!(json["client_data"], r#"{"type":"webauthn.create"}"#);
    let json: serde_json::Value =
        serde_json::from_str(&get_passkey_parameters(RequestId(8))).unwrap();
    assert_eq!(json["@type"], "getPasskeyParameters");
}

#[test]
fn the_row_opens_nothing_while_loading() {
    assert_eq!(PasskeysState::default().row_target(), None);
    assert_eq!(known(vec![]).row_target(), Some(RowTarget::About));
    assert_eq!(
        known(vec![key("a", "x")]).row_target(),
        Some(RowTarget::List)
    );
}

#[test]
fn a_fetched_list_stays_fresh_for_five_seconds() {
    let now = Instant::now();
    let mut state = PasskeysState::default();
    assert!(state.wants_fetch(now));
    state.requested_at = Some(now);
    assert!(!state.wants_fetch(now + Duration::from_secs(5)));
    assert!(state.wants_fetch(now + Duration::from_secs(6)));
    state.stale = true;
    assert!(state.wants_fetch(now));
}

#[test]
fn a_removal_forgets_the_passkey_and_asks_again() {
    let mut state = known(vec![key("a", "x"), key("b", "y")]);
    state.removing = Some("a".into());
    state.requested_at = Some(Instant::now());
    state.finish_remove("a");
    assert_eq!(state.list.len(), 1);
    assert!(state.find("a").is_none());
    assert!(!state.known);
    assert!(state.stale);
    assert!(state.removing.is_none());
    assert!(state.wants_fetch(Instant::now()));
}

#[test]
fn the_date_line_prefers_the_last_use() {
    let now = civil_at(1_790_631_720, 0); // 2026-09-28 21:42 UTC
    let mut passkey = key("a", "x");
    passkey.addition_date = 1_790_631_720 - 5 * 86_400;
    assert_eq!(
        passkey.date_line(|t| civil_at(t, 0), &now),
        "Created September 23 at 21:42"
    );
    passkey.last_usage_date = 1_790_631_720 - 3600;
    assert_eq!(
        passkey.date_line(|t| civil_at(t, 0), &now),
        "Last used September 28 at 20:42"
    );
}

#[test]
fn the_delete_question_names_the_passkey() {
    let (before, name, after) = delete_question("YubiKey");
    assert_eq!(before, "Once deleted, ");
    assert_eq!(name.as_deref(), Some("YubiKey"));
    assert!(after.starts_with(" can't be used to log in.\n\nDon't forget"));
    let (_, name, after) = delete_question("");
    assert!(name.is_none());
    assert!(after.starts_with("Once deleted, this passkey can't be used to log in."));
}

#[test]
fn reads_the_display_switch_from_the_app_config() {
    let config = |value: bool| {
        serde_json::json!({"@type": "jsonValueObject", "members": [
            {"@type": "jsonObjectMember", "key": "settings_display_passkeys",
             "value": {"@type": "jsonValueBoolean", "value": value}}
        ]})
    };
    assert!(display_allowed_from_config(&config(true)));
    assert!(!display_allowed_from_config(&config(false)));
    assert!(!display_allowed_from_config(
        &serde_json::json!({"@type": "jsonValueObject", "members": []})
    ));
    let with_max = serde_json::json!({"@type": "jsonValueObject", "members": [
        {"@type": "jsonObjectMember", "key": "passkeys_account_passkeys_max",
         "value": {"@type": "jsonValueNumber", "value": 5.0}}
    ]});
    assert_eq!(
        config_from_app_config(&with_max),
        PasskeysConfig {
            display: false,
            max: Some(5)
        }
    );
}
