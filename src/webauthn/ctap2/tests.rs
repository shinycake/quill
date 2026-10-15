//! CTAP2 against the scripted fake key (`fake.rs`).
use super::cbor::{as_bytes, as_i64, as_text, get_int_key, get_text_key, map};
use super::commands::{
    credential_id_from_auth_data, get_assertion, make_credential, none_attestation_object,
};
use super::ctaphid::{Assembly, frame};
use super::fake::{CID, CREDENTIAL_ID, FakeKey, Script};
use super::hid::parse_descriptor;
use super::pin::{PERMISSION_MAKE_CREDENTIAL, Protocol, SharedSecret, pin_token};
use super::*;
use crate::webauthn::options::{CreationOptions, RequestOptions, UserVerification};
use ciborium::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn script(edit: impl FnOnce(&mut Script)) -> Arc<Mutex<Script>> {
    let mut script = Script::default();
    edit(&mut script);
    Arc::new(Mutex::new(script))
}

fn open(script: &Arc<Mutex<Script>>, cancel: &AtomicBool) -> Authenticator {
    let mut ignore = |_: u8| {};
    let mut control = Control {
        cancel,
        deadline: Instant::now() + Duration::from_secs(5),
        on_keepalive: &mut ignore,
    };
    Authenticator::open(Box::new(FakeKey::new(script.clone())), &mut control).unwrap()
}

fn creation() -> CreationOptions {
    CreationOptions {
        rp_id: "telegram.org".into(),
        rp_name: "Telegram".into(),
        user_id: vec![9, 8, 7],
        user_name: "+15550100".into(),
        user_display_name: "Quill Test".into(),
        challenge: vec![1; 32],
        algorithms: vec![-257, -7],
        timeout_ms: 5_000,
    }
}

#[test]
fn messages_are_framed_and_reassembled() {
    let payload: Vec<u8> = (0..200u8).collect();
    let reports = frame(CID, 0x10, &payload, 64).unwrap();
    // 57 bytes in the first packet, 59 in each continuation.
    assert_eq!(reports.len(), 4);
    assert!(reports.iter().all(|r| r.len() == 64));
    assert_eq!(&reports[0][..7], &[1, 2, 3, 4, 0x90, 0, 200]);
    assert_eq!(reports[1][4], 0);
    assert_eq!(reports[3][4], 2);
    let mut assembly = Assembly::default();
    let mut done = None;
    for report in &reports {
        // Another channel's packet in between is skipped.
        assert!(
            assembly
                .feed(CID, &frame(7, 0x10, &[1], 64).unwrap()[0])
                .unwrap()
                .is_none()
        );
        done = assembly.feed(CID, report).unwrap();
    }
    assert_eq!(done, Some((0x10, payload)));
    // A continuation out of order is a protocol error.
    let mut assembly = Assembly::default();
    assembly.feed(CID, &reports[0]).unwrap();
    assert!(assembly.feed(CID, &reports[2]).is_err());
    // Too long for 128 continuation packets.
    assert_eq!(
        frame(CID, 0x10, &vec![0; 57 + 59 * 129], 64),
        Err(HidError::TooLong)
    );
}

#[test]
fn reads_the_fido_descriptor() {
    // A YubiKey's FIDO interface descriptor (HID usage tables, FIDO page).
    let descriptor = [
        0x06, 0xD0, 0xF1, 0x09, 0x01, 0xA1, 0x01, 0x09, 0x20, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x75,
        0x08, 0x95, 0x40, 0x81, 0x02, 0x09, 0x21, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x75, 0x08, 0x95,
        0x40, 0x91, 0x02, 0xC0,
    ];
    let summary = parse_descriptor(&descriptor);
    assert!(summary.is_fido());
    assert_eq!(summary.report_len(), Some(64));
    // A keyboard is not a FIDO device; garbage does not panic.
    let keyboard = parse_descriptor(&[0x05, 0x01, 0x09, 0x06, 0xA1, 0x01]);
    assert!(!keyboard.is_fido());
    let _ = parse_descriptor(&[0x27, 0xFF, 0xFE]);
    let _ = parse_descriptor(&[0xFE, 0xFF]);
}

#[test]
fn get_info_is_read_on_open() {
    let cancel = AtomicBool::new(false);
    let key = open(&script(|s| s.pin = Some("1234".into())), &cancel);
    let info = key.info();
    assert!(info.is_fido2());
    assert_eq!(info.client_pin, Some(true));
    assert!(info.pin_uv_auth_token);
    assert_eq!(info.pin_protocols, vec![2, 1]);
    let u2f = Info {
        versions: vec!["U2F_V2".into()],
        ..Default::default()
    };
    assert!(!u2f.is_fido2());
}

#[test]
fn make_credential_without_a_pin() {
    let cancel = AtomicBool::new(false);
    let fake = script(|_| {});
    let mut key = open(&fake, &cancel);
    let mut keepalives = Vec::new();
    let mut hear = |status: u8| keepalives.push(status);
    let mut control = Control {
        cancel: &cancel,
        deadline: Instant::now() + Duration::from_secs(5),
        on_keepalive: &mut hear,
    };
    let options = creation();
    let made = make_credential(&mut key, &options, &[3; 32], None, &mut control).unwrap();
    assert_eq!(made.credential_id, CREDENTIAL_ID.to_vec());
    assert_eq!(keepalives, vec![2, 2]);
    // The request: canonical keys, one algorithm (ES256), rk, no PIN.
    let seen = fake.lock().unwrap().seen.clone();
    let (cmd, params) = seen.last().unwrap();
    assert_eq!(*cmd, command::MAKE_CREDENTIAL);
    let params = map(params).unwrap();
    let keys: Vec<i64> = params.iter().filter_map(|(k, _)| as_i64(k)).collect();
    assert_eq!(keys, vec![1, 2, 3, 4, 7]);
    let user = map(get_int_key(params, 3).unwrap()).unwrap();
    assert_eq!(
        as_bytes(get_text_key(user, "id").unwrap()),
        Some(&[9, 8, 7][..])
    );
    let alg = &super::cbor::as_array(get_int_key(params, 4).unwrap()).unwrap()[0];
    assert_eq!(
        get_text_key(map(alg).unwrap(), "alg").and_then(as_i64),
        Some(-7)
    );
    // The bytes on the wire match the CTAP2 canonical encoding.
    let encoded = super::cbor::encode(&seen.last().unwrap().1).unwrap();
    assert_eq!(&encoded[..3], &[0xA5, 0x01, 0x58]);
}

#[test]
fn pin_protocols_round_trip() {
    let mut rng = rand::rngs::OsRng;
    for protocol in [Protocol::One, Protocol::Two] {
        let secret = SharedSecret::derive(protocol, &[0x42; 32]);
        let plain = [0x11u8; 32];
        let encrypted = secret.encrypt(&plain, &mut rng).unwrap();
        let expected_len = if protocol == Protocol::Two { 48 } else { 32 };
        assert_eq!(encrypted.len(), expected_len);
        assert_eq!(secret.decrypt(&encrypted).unwrap().as_slice(), &plain);
        // Not a whole number of blocks: refused, no panic.
        assert!(secret.encrypt(&[1, 2, 3], &mut rng).is_none());
        assert!(secret.decrypt(&[1, 2, 3]).is_none());
        let tag = secret.authenticate(b"message");
        assert_eq!(tag.len(), if protocol == Protocol::Two { 32 } else { 16 });
    }
    // Protocol one's key is SHA-256 of the shared point's x coordinate,
    // and its IV is zero: the same input encrypts the same way twice.
    let one = SharedSecret::derive(Protocol::One, &[0x42; 32]);
    assert_eq!(
        one.encrypt(&[0; 16], &mut rng),
        one.encrypt(&[0; 16], &mut rng)
    );
    assert_eq!(Protocol::choose(&[1]), Protocol::One);
    assert_eq!(Protocol::choose(&[]), Protocol::One);
    assert_eq!(Protocol::choose(&[1, 2]), Protocol::Two);
}

#[test]
fn a_pin_becomes_a_token_with_permissions() {
    for (protocols, with_permissions) in [(vec![2, 1], true), (vec![1], false)] {
        let cancel = AtomicBool::new(false);
        let fake = script(|s| {
            s.pin = Some("4321".into());
            s.pin_protocols = protocols.clone();
            s.pin_uv_auth_token = with_permissions;
        });
        let mut key = open(&fake, &cancel);
        let mut ignore = |_: u8| {};
        let mut control = Control {
            cancel: &cancel,
            deadline: Instant::now() + Duration::from_secs(5),
            on_keepalive: &mut ignore,
        };
        let mut rng = rand::rngs::OsRng;
        let token = pin_token(
            &mut key,
            "4321",
            PERMISSION_MAKE_CREDENTIAL,
            "telegram.org",
            &mut rng,
            &mut control,
        )
        .unwrap();
        assert_eq!(token.token.as_slice(), &super::fake::TOKEN);
        let seen = fake.lock().unwrap().seen.clone();
        let (_, request) = seen.last().unwrap();
        let request = map(request).unwrap();
        assert_eq!(
            get_int_key(request, 2).and_then(as_i64),
            Some(if with_permissions { 9 } else { 5 })
        );
        assert_eq!(
            get_int_key(request, 10).and_then(as_text),
            with_permissions.then_some("telegram.org")
        );
        // The token signs makeCredential, which the key accepts.
        let made = make_credential(&mut key, &creation(), &[3; 32], Some(&token), &mut control);
        assert!(made.is_ok(), "{made:?}");
        // A wrong PIN is refused with PIN_INVALID.
        let wrong = pin_token(
            &mut key,
            "0000",
            PERMISSION_MAKE_CREDENTIAL,
            "telegram.org",
            &mut rng,
            &mut control,
        );
        assert_eq!(wrong.err(), Some(CtapError::Status(status::PIN_INVALID)));
    }
}

#[test]
fn a_key_with_a_pin_asks_for_it() {
    let cancel = AtomicBool::new(false);
    let fake = script(|s| s.pin = Some("1234".into()));
    let mut key = open(&fake, &cancel);
    let mut ignore = |_: u8| {};
    let mut control = Control {
        cancel: &cancel,
        deadline: Instant::now() + Duration::from_secs(5),
        on_keepalive: &mut ignore,
    };
    let made = make_credential(&mut key, &creation(), &[3; 32], None, &mut control);
    assert_eq!(made, Err(CtapError::Status(status::PIN_REQUIRED)));
}

#[test]
fn get_assertion_reads_the_first_assertion() {
    let cancel = AtomicBool::new(false);
    let fake = script(|_| {});
    let mut key = open(&fake, &cancel);
    let mut ignore = |_: u8| {};
    let mut control = Control {
        cancel: &cancel,
        deadline: Instant::now() + Duration::from_secs(5),
        on_keepalive: &mut ignore,
    };
    let options = RequestOptions {
        rp_id: "telegram.org".into(),
        challenge: vec![2; 32],
        allow_credentials: vec![vec![0xAB; 4], CREDENTIAL_ID.to_vec()],
        user_verification: UserVerification::Discouraged,
        timeout_ms: 5_000,
    };
    let got = get_assertion(&mut key, &options, &[4; 32], None, &mut control).unwrap();
    assert_eq!(got.credential_id, CREDENTIAL_ID.to_vec());
    assert_eq!(got.user_handle, vec![1, 2, 3, 4]);
    assert_eq!(got.signature, vec![0x30, 0x44, 0x02, 0x20]);
    let seen = fake.lock().unwrap().seen.clone();
    let params = map(&seen.last().unwrap().1).unwrap().to_vec();
    let allow = super::cbor::as_array(get_int_key(&params, 3).unwrap()).unwrap();
    assert_eq!(allow.len(), 2);
    let options = map(get_int_key(&params, 5).unwrap()).unwrap();
    assert_eq!(get_text_key(options, "uv"), Some(&Value::Bool(false)));
    // Errors come back as their status.
    fake.lock().unwrap().fail_next = Some(status::NO_CREDENTIALS);
    let options = RequestOptions {
        rp_id: "telegram.org".into(),
        ..Default::default()
    };
    assert_eq!(
        get_assertion(&mut key, &options, &[4; 32], None, &mut control),
        Err(CtapError::Status(status::NO_CREDENTIALS))
    );
}

#[test]
fn cancel_while_waiting_for_a_touch() {
    let cancel = Arc::new(AtomicBool::new(false));
    let fake = script(|s| s.hold = true);
    let mut key = open(&fake, &cancel);
    let flag = cancel.clone();
    let mut heard = 0;
    let mut hear = |_: u8| {
        heard += 1;
        if heard == 3 {
            flag.store(true, Ordering::SeqCst);
        }
    };
    let mut control = Control {
        cancel: &cancel,
        deadline: Instant::now() + Duration::from_secs(5),
        on_keepalive: &mut hear,
    };
    let made = make_credential(&mut key, &creation(), &[3; 32], None, &mut control);
    assert_eq!(made, Err(CtapError::Hid(HidError::Cancelled)));
    assert_eq!(fake.lock().unwrap().cancels, 1);
    // The deadline cancels the same way.
    let cancel = AtomicBool::new(false);
    let fake = script(|s| s.hold = true);
    let mut key = open(&fake, &cancel);
    let mut ignore = |_: u8| {};
    let mut control = Control {
        cancel: &cancel,
        deadline: Instant::now() + Duration::from_millis(50),
        on_keepalive: &mut ignore,
    };
    let made = make_credential(&mut key, &creation(), &[3; 32], None, &mut control);
    assert_eq!(made, Err(CtapError::Hid(HidError::Timeout)));
    assert_eq!(fake.lock().unwrap().cancels, 1);
}

#[test]
fn the_attestation_object_is_tdesktops_none_format() {
    let auth_data = vec![0xEE; 30];
    let object = none_attestation_object(&auth_data);
    let decoded: Value = ciborium::from_reader(object.as_slice()).unwrap();
    let entries = map(&decoded).unwrap();
    let keys: Vec<&str> = entries.iter().filter_map(|(k, _)| as_text(k)).collect();
    assert_eq!(keys, vec!["fmt", "attStmt", "authData"]);
    assert_eq!(get_text_key(entries, "fmt").and_then(as_text), Some("none"));
    assert_eq!(
        get_text_key(entries, "authData").and_then(as_bytes),
        Some(auth_data.as_slice())
    );
    // Each byte-string header length, as tdesktop writes it below 64 KiB.
    assert_eq!(none_attestation_object(&[1; 5])[28], 0x45);
    assert_eq!(&none_attestation_object(&[1; 200])[28..30], &[0x58, 200]);
    assert_eq!(
        &none_attestation_object(&[1; 300])[28..31],
        &[0x59, 0x01, 0x2C]
    );
}

#[test]
fn credential_id_is_bounds_checked() {
    let mut auth_data = vec![0; 32];
    auth_data.push(0x45);
    auth_data.extend_from_slice(&[0; 4 + 16]);
    auth_data.extend_from_slice(&[0, 3, 7, 8, 9]);
    assert_eq!(
        credential_id_from_auth_data(&auth_data),
        Some(vec![7, 8, 9])
    );
    // Claims more bytes than there are, or no attested data: none.
    auth_data[54] = 200;
    assert_eq!(credential_id_from_auth_data(&auth_data), None);
    assert_eq!(credential_id_from_auth_data(&[0; 37]), None);
    assert_eq!(credential_id_from_auth_data(&[]), None);
}
