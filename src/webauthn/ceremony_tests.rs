//! Whole ceremonies on the worker thread against the fake key.
use super::*;
use crate::webauthn::ctap2::fake::{CREDENTIAL_ID, FakeSource, Script};
use std::sync::Mutex;

fn fake_source(
    script: Script,
    absent: usize,
) -> (Box<FakeSource>, Arc<Mutex<Script>>, Arc<Mutex<usize>>) {
    let script = Arc::new(Mutex::new(script));
    let looks = Arc::new(Mutex::new(0));
    (
        Box::new(FakeSource {
            script: script.clone(),
            absent,
            looks: looks.clone(),
        }),
        script,
        looks,
    )
}

fn options(timeout_ms: u32) -> CreationOptions {
    CreationOptions {
        rp_id: "telegram.org".into(),
        rp_name: "Telegram".into(),
        user_id: vec![1, 2],
        user_name: "+15550100".into(),
        user_display_name: "Quill".into(),
        challenge: vec![0xde, 0xad, 0xbe, 0xef],
        algorithms: vec![-7],
        timeout_ms,
    }
}

/// The next event, waiting up to five seconds.
fn next(ceremony: &mut Ceremony) -> CeremonyEvent {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(event) = ceremony.poll().into_iter().next() {
            return event;
        }
        assert!(Instant::now() < until, "no event");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn registered(event: CeremonyEvent) -> Registration {
    match event {
        CeremonyEvent::Registered(registration) => registration,
        other => panic!("not registered: {other:?}"),
    }
}

#[test]
fn registers_without_a_pin() {
    let (source, _, looks) = fake_source(Script::default(), 2);
    let mut ceremony = Ceremony::register(options(5_000), source);
    let registration = registered(next(&mut ceremony));
    assert_eq!(registration.credential_id, CREDENTIAL_ID.to_vec());
    assert_eq!(
        registration.client_data_json,
        r#"{"challenge":"3q2-7w","crossOrigin":false,"origin":"https://telegram.org","type":"webauthn.create"}"#
    );
    assert_eq!(
        &registration.attestation_object[..10],
        b"\xA3\x63fmt\x64none"
    );
    // The key was plugged in on the third look.
    assert_eq!(*looks.lock().unwrap(), 3);
    // Nothing after the end.
    assert!(ceremony.poll().is_empty());
}

#[test]
fn asks_for_the_pin_and_again_after_a_wrong_one() {
    let (source, script, _) = fake_source(
        Script {
            pin: Some("123456".into()),
            ..Default::default()
        },
        0,
    );
    let mut ceremony = Ceremony::register(options(5_000), source);
    assert_eq!(
        next(&mut ceremony),
        CeremonyEvent::NeedPin { invalid: false }
    );
    ceremony.submit_pin(Zeroizing::new("000000".into()));
    assert_eq!(
        next(&mut ceremony),
        CeremonyEvent::NeedPin { invalid: true }
    );
    ceremony.submit_pin(Zeroizing::new("123456".into()));
    let registration = registered(next(&mut ceremony));
    assert_eq!(registration.credential_id, CREDENTIAL_ID.to_vec());
    // The last makeCredential carried a pinUvAuthParam with protocol 2.
    let seen = script.lock().unwrap().seen.clone();
    let (_, params) = seen.last().unwrap();
    let params = crate::webauthn::ctap2::cbor::map(params).unwrap();
    assert!(crate::webauthn::ctap2::cbor::get_int_key(params, 8).is_some());
    assert_eq!(
        crate::webauthn::ctap2::cbor::get_int_key(params, 9)
            .and_then(crate::webauthn::ctap2::cbor::as_i64),
        Some(2)
    );
}

#[test]
fn a_blocked_pin_ends_the_ceremony() {
    let (source, _, _) = fake_source(
        Script {
            pin: Some("123456".into()),
            pin_retries: 1,
            ..Default::default()
        },
        0,
    );
    let mut ceremony = Ceremony::register(options(5_000), source);
    assert_eq!(
        next(&mut ceremony),
        CeremonyEvent::NeedPin { invalid: false }
    );
    ceremony.submit_pin(Zeroizing::new("000000".into()));
    assert_eq!(
        next(&mut ceremony),
        CeremonyEvent::Failed(CeremonyError::PinBlocked)
    );
}

#[test]
fn no_key_until_the_timeout() {
    let (source, _, looks) = fake_source(Script::default(), usize::MAX);
    let mut ceremony = Ceremony::register(options(1_200), source);
    assert_eq!(
        next(&mut ceremony),
        CeremonyEvent::Failed(CeremonyError::NoDevice)
    );
    // Looked every half second.
    assert!((3..=4).contains(&*looks.lock().unwrap()));
}

#[test]
fn cancel_while_asking_for_the_pin() {
    let (source, _, _) = fake_source(
        Script {
            pin: Some("1".into()),
            ..Default::default()
        },
        0,
    );
    let mut ceremony = Ceremony::register(options(5_000), source);
    assert_eq!(
        next(&mut ceremony),
        CeremonyEvent::NeedPin { invalid: false }
    );
    ceremony.cancel();
    assert_eq!(
        next(&mut ceremony),
        CeremonyEvent::Failed(CeremonyError::Cancelled)
    );
}

#[test]
fn cancel_while_waiting_for_a_touch() {
    let (source, script, _) = fake_source(
        Script {
            hold: true,
            ..Default::default()
        },
        0,
    );
    let mut ceremony = Ceremony::register(options(5_000), source);
    std::thread::sleep(Duration::from_millis(100));
    ceremony.cancel();
    assert_eq!(
        next(&mut ceremony),
        CeremonyEvent::Failed(CeremonyError::Cancelled)
    );
    assert_eq!(script.lock().unwrap().cancels, 1);
}

#[test]
fn key_errors_map_like_tdesktop() {
    for (code, expected) in [
        (status::OPERATION_DENIED, None),
        (status::CREDENTIAL_EXCLUDED, None),
        (status::PIN_AUTH_BLOCKED, Some(CeremonyError::PinBlocked)),
    ] {
        let (source, _, _) = fake_source(
            Script {
                fail_next: Some(code),
                ..Default::default()
            },
            0,
        );
        let mut ceremony = Ceremony::register(options(5_000), source);
        match (next(&mut ceremony), expected) {
            (CeremonyEvent::Failed(error), Some(expected)) => assert_eq!(error, expected),
            (CeremonyEvent::Failed(CeremonyError::Other(_)), None) => {}
            (other, _) => panic!("{code:#x}: {other:?}"),
        }
    }
    // A U2F-only key cannot make a passkey.
    let (source, _, _) = fake_source(
        Script {
            versions: vec!["U2F_V2"],
            ..Default::default()
        },
        0,
    );
    let mut ceremony = Ceremony::register(options(5_000), source);
    assert!(matches!(
        next(&mut ceremony),
        CeremonyEvent::Failed(CeremonyError::Other(_))
    ));
}

#[test]
fn signs_in_and_reports_missing_credentials() {
    let request = RequestOptions {
        rp_id: "telegram.org".into(),
        challenge: vec![0xde, 0xad, 0xbe, 0xef],
        timeout_ms: 5_000,
        ..Default::default()
    };
    let (source, _, _) = fake_source(Script::default(), 0);
    let mut ceremony = Ceremony::login(request.clone(), source);
    match next(&mut ceremony) {
        CeremonyEvent::SignedIn(assertion) => {
            assert_eq!(assertion.credential_id, CREDENTIAL_ID.to_vec());
            assert_eq!(assertion.user_handle, vec![1, 2, 3, 4]);
            assert!(
                assertion
                    .client_data_json
                    .contains("\"type\":\"webauthn.get\"")
            );
        }
        other => panic!("{other:?}"),
    }
    let (source, _, _) = fake_source(
        Script {
            fail_next: Some(status::NO_CREDENTIALS),
            ..Default::default()
        },
        0,
    );
    let mut ceremony = Ceremony::login(request, source);
    assert_eq!(
        next(&mut ceremony),
        CeremonyEvent::Failed(CeremonyError::NoCredentials)
    );
}

#[test]
fn dropping_the_handle_cancels() {
    let (source, script, _) = fake_source(
        Script {
            hold: true,
            ..Default::default()
        },
        0,
    );
    let ceremony = Ceremony::register(options(5_000), source);
    std::thread::sleep(Duration::from_millis(100));
    drop(ceremony);
    let until = Instant::now() + Duration::from_secs(3);
    while script.lock().unwrap().cancels == 0 {
        assert!(Instant::now() < until, "the key was not cancelled");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The Windows shape: one blocking call that only `on_cancel` ends
/// (`WebAuthNCancelCurrentOperation`), its outcome the one event.
#[test]
fn a_system_ceremony_is_cancelled_through_its_hook() {
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    let stop_tx = Mutex::new(stop_tx);
    let mut ceremony = Ceremony::spawn_system(
        move || match stop_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(()) => CeremonyEvent::Failed(CeremonyError::Cancelled),
            Err(_) => CeremonyEvent::Failed(CeremonyError::Other("never cancelled".into())),
        },
        Arc::new(move || {
            if let Ok(tx) = stop_tx.lock() {
                let _ = tx.send(());
            }
        }),
    );
    assert!(ceremony.poll().is_empty());
    ceremony.cancel();
    assert_eq!(
        next(&mut ceremony),
        CeremonyEvent::Failed(CeremonyError::Cancelled)
    );
}

#[test]
fn a_system_ceremony_reports_its_result() {
    let mut ceremony = Ceremony::spawn_system(
        || {
            CeremonyEvent::Registered(Registration {
                credential_id: vec![1],
                attestation_object: vec![2],
                client_data_json: "{}".into(),
            })
        },
        Arc::new(|| {}),
    );
    assert!(matches!(next(&mut ceremony), CeremonyEvent::Registered(_)));
    // Nothing more once it finished, and a late cancel is harmless.
    ceremony.cancel();
    assert!(ceremony.poll().is_empty());
}
