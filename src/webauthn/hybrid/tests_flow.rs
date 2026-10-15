//! Whole phone ceremonies on the worker thread: a fake scanner, the
//! scripted phone behind a websocket on localhost, and the fake USB key.

use super::super::fake_phone::{
    FakePhone, FakeScanner, PhoneScript, advert, get_assertion_answer, make_credential_answer,
    make_eid, scan_qr, serve,
};
use super::super::tunnel::{PlainConnector, SUBPROTOCOL};
use super::super::{Environment, ble::ScanEvent, eid, login, qr, register};
use super::{creation, request};
use crate::webauthn::ctap2::fake::{CREDENTIAL_ID, FakeSource, Script};
use crate::webauthn::{Ceremony, CeremonyError, CeremonyEvent, HybridQr};
use rand::SeedableRng;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Rig {
    events: Arc<Mutex<Option<Sender<ScanEvent>>>>,
    stopped: Arc<Mutex<bool>>,
    paths: Arc<Mutex<Vec<String>>>,
    key_looks: Arc<Mutex<usize>>,
}

/// An environment with a fake scanner (`available` as it reports), a
/// connector to `phone` (or nowhere), and a USB key present after
/// `key_absent` looks (`None`: never).
fn environment(
    available: Option<bool>,
    phone: Option<std::net::SocketAddr>,
    key_absent: Option<usize>,
) -> (Environment, Rig) {
    let events = Arc::new(Mutex::new(None));
    let stopped = Arc::new(Mutex::new(false));
    let paths = Arc::new(Mutex::new(Vec::new()));
    let key_looks = Arc::new(Mutex::new(0));
    let scanner = FakeScanner {
        available,
        events: events.clone(),
        stopped: stopped.clone(),
    };
    let env = Environment {
        scanner: Some(Box::new(move || Some(Box::new(scanner) as Box<_>))),
        connector: Box::new(PlainConnector {
            addr: phone.unwrap_or_else(|| "127.0.0.1:9".parse().unwrap()),
            paths: paths.clone(),
        }),
        keys: Box::new(FakeSource {
            script: Arc::new(Mutex::new(Script::default())),
            absent: key_absent.unwrap_or(usize::MAX),
            looks: key_looks.clone(),
        }),
        rng: Box::new(rand::rngs::StdRng::seed_from_u64(7)),
        now_unix: 1_760_000_000,
        bluetooth_probe: Duration::from_millis(300),
    };
    (
        env,
        Rig {
            events,
            stopped,
            paths,
            key_looks,
        },
    )
}

/// A ceremony and the events polled but not looked at yet.
struct Watched {
    ceremony: Ceremony,
    pending: std::collections::VecDeque<CeremonyEvent>,
}

impl std::ops::Deref for Watched {
    type Target = Ceremony;
    fn deref(&self) -> &Ceremony {
        &self.ceremony
    }
}

fn watch(ceremony: Ceremony) -> Watched {
    Watched {
        ceremony,
        pending: Default::default(),
    }
}

/// The next event, waiting up to ten seconds.
fn next(watched: &mut Watched) -> CeremonyEvent {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(event) = watched.pending.pop_front() {
            return event;
        }
        watched.pending.extend(watched.ceremony.poll());
        if watched.pending.is_empty() {
            assert!(Instant::now() < until, "no event");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

fn qr_of(event: CeremonyEvent) -> String {
    match event {
        CeremonyEvent::Hybrid {
            qr: Some(HybridQr(text)),
        } => text,
        other => panic!("not a QR sheet: {other:?}"),
    }
}

/// Scan the QR, start the phone's tunnel and advertise to the scanner.
fn phone_appears(rig: &Rig, qr_text: &str, script: PhoneScript) -> (FakePhone, [u8; 16]) {
    let scanned = scan_qr(qr_text);
    let eid = make_eid([0xab, 0xcd, 0xef], 0);
    let phone = serve(scanned.clone(), eid, script);
    let sender = rig.events.lock().unwrap().clone().expect("scanning");
    // Someone else's phone first: ignored.
    let mut other = scanned.clone();
    other.secret = [9; 16];
    sender
        .send(ScanEvent::Advert(advert(&other, &eid)))
        .unwrap();
    sender
        .send(ScanEvent::Advert(advert(&scanned, &eid)))
        .unwrap();
    (phone, eid)
}

#[test]
fn registers_on_a_phone() {
    let phone_script = PhoneScript {
        revision: 1,
        update_first: true,
        features: Some(vec!["ctap", "dc"]),
        status: 0,
        answer: make_credential_answer(&[0xc1, 0xc2, 0xc3]),
    };
    // The phone's address is only known once it serves.
    let (env, rig, qr_holder) = late_environment(Some(true));
    let options = creation();
    let mut ceremony = watch(register(options.clone(), env));
    let qr_text = qr_of(next(&mut ceremony));
    let scanned = scan_qr(&qr_text);
    assert_eq!(scanned.request, "mc");
    assert_eq!(scanned.domains, 2);
    assert_eq!(scanned.timestamp, 1_760_000_000);
    let (phone, _) = phone_appears(&rig, &qr_text, phone_script);
    *qr_holder.lock().unwrap() = Some(phone.addr);
    assert_eq!(next(&mut ceremony), CeremonyEvent::PhoneConnecting);
    assert_eq!(next(&mut ceremony), CeremonyEvent::PhoneContinue);
    let registration = match next(&mut ceremony) {
        CeremonyEvent::Registered(registration) => registration,
        other => panic!("not registered: {other:?}"),
    };
    assert_eq!(registration.credential_id, vec![0xc1, 0xc2, 0xc3]);
    // tdesktop sends a "none" attestation whatever the phone made.
    assert_eq!(
        &registration.attestation_object[..10],
        b"\xA3\x63fmt\x64none"
    );
    assert_eq!(registration.client_data_json, options.client_data_json());
    assert!(*rig.stopped.lock().unwrap(), "scanner stopped");

    let log = phone.done.join().unwrap();
    let tunnel_id = qr::derive::<16>(&scanned.secret, &[], qr::Derived::TunnelId);
    assert_eq!(
        log.path,
        format!(
            "/cable/connect/abcdef/{}",
            eid::hex_lower(tunnel_id.as_ref())
        )
    );
    assert_eq!(
        rig.paths.lock().unwrap()[0],
        format!("cable.ua5v.com{}", log.path)
    );
    assert_eq!(log.subprotocol, SUBPROTOCOL);
    // The request: the type byte, then makeCredential with every algorithm.
    let hash = super::super::crypto::sha256(options.client_data_json().as_bytes());
    let mut expected = vec![1];
    expected.extend(super::super::messages::make_credential_request(&hash, &options).unwrap());
    assert_eq!(log.request, expected);
}

/// A connector whose address is filled in once the phone serves.
struct LateConnector {
    addr: Arc<Mutex<Option<std::net::SocketAddr>>>,
    paths: Arc<Mutex<Vec<String>>>,
}

impl super::super::tunnel::Connector for LateConnector {
    fn connect(
        &mut self,
        domain: &str,
        path: &str,
        stop: &super::super::tunnel::Stop<'_>,
    ) -> Result<Box<dyn super::super::tunnel::Tunnel>, super::super::tunnel::TunnelError> {
        let addr = self.addr.lock().unwrap().expect("phone address");
        PlainConnector {
            addr,
            paths: self.paths.clone(),
        }
        .connect(domain, path, stop)
    }
}

fn late_environment(
    available: Option<bool>,
) -> (Environment, Rig, Arc<Mutex<Option<std::net::SocketAddr>>>) {
    let (mut env, rig) = environment(available, None, None);
    let addr = Arc::new(Mutex::new(None));
    env.connector = Box::new(LateConnector {
        addr: addr.clone(),
        paths: rig.paths.clone(),
    });
    (env, rig, addr)
}

#[test]
fn signs_in_with_an_older_phone() {
    let (env, rig, addr) = late_environment(Some(true));
    let options = request(vec![vec![0xc1]]);
    let mut ceremony = watch(login(options.clone(), env));
    let qr_text = qr_of(next(&mut ceremony));
    assert_eq!(scan_qr(&qr_text).request, "ga");
    let (phone, _) = phone_appears(
        &rig,
        &qr_text,
        PhoneScript {
            revision: 0,
            update_first: false,
            features: None,
            status: 0,
            answer: get_assertion_answer(&[0xc1], &[0x0a, 0x0b]),
        },
    );
    *addr.lock().unwrap() = Some(phone.addr);
    assert_eq!(next(&mut ceremony), CeremonyEvent::PhoneConnecting);
    assert_eq!(next(&mut ceremony), CeremonyEvent::PhoneContinue);
    let assertion = match next(&mut ceremony) {
        CeremonyEvent::SignedIn(assertion) => assertion,
        other => panic!("not signed in: {other:?}"),
    };
    assert_eq!(assertion.credential_id, vec![0xc1]);
    assert_eq!(assertion.user_handle, vec![0x0a, 0x0b]);
    assert_eq!(assertion.signature, vec![0x30, 0x44, 0x02]);
    assert_eq!(assertion.authenticator_data, vec![0x5a; 37]);
    assert_eq!(assertion.client_data_json, options.client_data_json());
    // Revision 0: no type byte in front of getAssertion.
    let log = phone.done.join().unwrap();
    assert_eq!(log.request.first(), Some(&0x02));
}

#[test]
fn a_phone_error_ends_with_the_phone_error() {
    let (env, rig, addr) = late_environment(Some(true));
    let mut ceremony = watch(register(creation(), env));
    let qr_text = qr_of(next(&mut ceremony));
    let (phone, _) = phone_appears(
        &rig,
        &qr_text,
        PhoneScript {
            revision: 1,
            update_first: false,
            features: Some(vec!["ctap"]),
            status: 0x27,
            answer: make_credential_answer(&[1]),
        },
    );
    *addr.lock().unwrap() = Some(phone.addr);
    assert_eq!(next(&mut ceremony), CeremonyEvent::PhoneConnecting);
    assert_eq!(next(&mut ceremony), CeremonyEvent::PhoneContinue);
    assert_eq!(
        next(&mut ceremony),
        CeremonyEvent::Failed(CeremonyError::Phone)
    );
    let _ = phone.done.join();
}

#[test]
#[expect(
    clippy::result_large_err,
    reason = "tungstenite's handshake callback returns its own ErrorResponse"
)]
fn a_phone_without_ctap_is_refused() {
    let (env, rig, addr) = late_environment(Some(true));
    let mut ceremony = watch(register(creation(), env));
    let qr_text = qr_of(next(&mut ceremony));
    let scanned = scan_qr(&qr_text);
    let eid = make_eid([1, 2, 3], 0);
    // The phone only offers "dc"; it never gets a request.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let phone_addr = listener.local_addr().unwrap();
    let scanned_in = scanned.clone();
    let phone = std::thread::spawn(move || {
        use super::super::noise::respond;
        let (stream, _) = listener.accept().unwrap();
        let mut ws = tungstenite::accept_hdr(
            stream,
            |_: &_, mut response: tungstenite::handshake::server::Response| {
                response
                    .headers_mut()
                    .insert("Sec-WebSocket-Protocol", SUBPROTOCOL.parse().unwrap());
                Ok(response)
            },
        )
        .unwrap();
        let first = loop {
            if let tungstenite::Message::Binary(data) = ws.read().unwrap() {
                break data.to_vec();
            }
        };
        let psk = qr::derive::<32>(&scanned_in.secret, &eid, qr::Derived::Psk);
        let key = p256::SecretKey::from_slice(&[0x44; 32]).unwrap();
        let (response, handshake) = respond(&psk, &scanned_in.identity_x962, &first, &key).unwrap();
        ws.send(tungstenite::Message::Binary(response.into()))
            .unwrap();
        let mut crypter = handshake.crypter;
        let post = crate::webauthn::ctap2::cbor::encode(&ciborium::Value::Map(vec![(
            ciborium::Value::Integer(3.into()),
            ciborium::Value::Array(vec![ciborium::Value::Text("dc".into())]),
        )]))
        .unwrap();
        ws.send(tungstenite::Message::Binary(
            crypter.encrypt(&post).unwrap().into(),
        ))
        .unwrap();
        // Quill closes instead of sending a request.
        matches!(ws.read(), Ok(tungstenite::Message::Close(_)) | Err(_))
    });
    *addr.lock().unwrap() = Some(phone_addr);
    let sender = rig.events.lock().unwrap().clone().unwrap();
    sender
        .send(ScanEvent::Advert(advert(&scanned, &eid)))
        .unwrap();
    assert_eq!(next(&mut ceremony), CeremonyEvent::PhoneConnecting);
    assert_eq!(
        next(&mut ceremony),
        CeremonyEvent::Failed(CeremonyError::Phone)
    );
    assert!(phone.join().unwrap(), "closed without a request");
}

#[test]
fn bluetooth_off_shows_no_code() {
    let (env, _rig) = environment(Some(false), None, None);
    let mut ceremony = watch(register(creation(), env));
    assert_eq!(next(&mut ceremony), CeremonyEvent::Hybrid { qr: None });
    ceremony.cancel();
    assert_eq!(
        next(&mut ceremony),
        CeremonyEvent::Failed(CeremonyError::Cancelled)
    );
}

#[test]
fn a_silent_scanner_reads_as_bluetooth_off() {
    // The scanner never says; after the probe the sheet has no code.
    let (env, _rig) = environment(None, None, None);
    let started = Instant::now();
    let mut ceremony = watch(register(creation(), env));
    assert_eq!(next(&mut ceremony), CeremonyEvent::Hybrid { qr: None });
    assert!(started.elapsed() >= Duration::from_millis(250));
}

#[test]
fn no_scanner_at_all_reads_as_bluetooth_off() {
    let (mut env, _rig) = environment(Some(true), None, None);
    env.scanner = Some(Box::new(|| None));
    let mut ceremony = watch(register(creation(), env));
    assert_eq!(next(&mut ceremony), CeremonyEvent::Hybrid { qr: None });
}

#[test]
fn security_key_without_a_key_stays_on_the_code() {
    let (env, _rig) = environment(Some(true), None, None);
    let mut ceremony = watch(register(creation(), env));
    let _ = qr_of(next(&mut ceremony));
    ceremony.use_security_key();
    assert_eq!(next(&mut ceremony), CeremonyEvent::NoKey);
    ceremony.use_security_key();
    assert_eq!(next(&mut ceremony), CeremonyEvent::NoKey);
}

#[test]
fn security_key_hands_over_to_the_key() {
    let (env, rig) = environment(Some(true), None, Some(0));
    let mut ceremony = watch(register(creation(), env));
    let _ = qr_of(next(&mut ceremony));
    ceremony.use_security_key();
    assert_eq!(next(&mut ceremony), CeremonyEvent::UseKey);
    let registration = match next(&mut ceremony) {
        CeremonyEvent::Registered(registration) => registration,
        other => panic!("not registered: {other:?}"),
    };
    assert_eq!(registration.credential_id, CREDENTIAL_ID.to_vec());
    assert!(*rig.stopped.lock().unwrap(), "scanner stopped");
    // One look to see the key is there, one to use it.
    assert_eq!(*rig.key_looks.lock().unwrap(), 2);
}

#[test]
fn the_code_expires_with_the_request() {
    let (env, _rig) = environment(Some(true), None, None);
    let mut options = creation();
    options.timeout_ms = 400;
    let mut ceremony = watch(register(options, env));
    let _ = qr_of(next(&mut ceremony));
    assert_eq!(
        next(&mut ceremony),
        CeremonyEvent::Failed(CeremonyError::Phone)
    );
}

#[test]
fn an_unreachable_tunnel_is_a_phone_error() {
    // Port 9 on localhost: nothing listens.
    let (env, rig) = environment(Some(true), None, None);
    let mut ceremony = watch(register(creation(), env));
    let qr_text = qr_of(next(&mut ceremony));
    let scanned = scan_qr(&qr_text);
    let eid = make_eid([1, 2, 3], 1);
    let sender = rig.events.lock().unwrap().clone().unwrap();
    sender
        .send(ScanEvent::Advert(advert(&scanned, &eid)))
        .unwrap();
    assert_eq!(next(&mut ceremony), CeremonyEvent::PhoneConnecting);
    assert_eq!(
        next(&mut ceremony),
        CeremonyEvent::Failed(CeremonyError::Phone)
    );
    assert_eq!(
        rig.paths.lock().unwrap()[0].split('/').next(),
        Some("cable.auth.com")
    );
}

#[test]
fn cancel_while_connecting() {
    // A server that accepts and never upgrades.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let silent = listener.local_addr().unwrap();
    let (env, rig, addr) = late_environment(Some(true));
    *addr.lock().unwrap() = Some(silent);
    let mut ceremony = watch(register(creation(), env));
    let qr_text = qr_of(next(&mut ceremony));
    let scanned = scan_qr(&qr_text);
    let eid = make_eid([1, 2, 3], 0);
    let sender = rig.events.lock().unwrap().clone().unwrap();
    sender
        .send(ScanEvent::Advert(advert(&scanned, &eid)))
        .unwrap();
    assert_eq!(next(&mut ceremony), CeremonyEvent::PhoneConnecting);
    let _held = listener.accept().unwrap();
    ceremony.cancel();
    assert_eq!(
        next(&mut ceremony),
        CeremonyEvent::Failed(CeremonyError::Cancelled)
    );
}
