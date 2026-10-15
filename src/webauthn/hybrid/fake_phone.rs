//! A scripted phone for the hybrid tests: reads Quill's QR code the way a
//! phone's camera would, makes the Bluetooth advertisement, and answers
//! as the other end of a websocket tunnel served on localhost.

use super::ble::{ScanEvent, Scanner};
use super::crypto::x962;
use super::eid::{EID_KEY_LEN, Eid, encrypt_advert};
use super::messages::decode_exact;
use super::noise::respond;
use super::qr::{self, Derived, SECRET_LEN, digits_to_bytes};
use super::tunnel::SUBPROTOCOL;
use crate::webauthn::ctap2::cbor::{
    as_bytes, as_i64, as_text, bytes, encode, get_int_key, int_entry, map, text, text_entry,
};
use ciborium::Value;
use p256::{PublicKey, SecretKey};
use std::net::{SocketAddr, TcpListener};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use tungstenite::Message;
use tungstenite::handshake::server::{Request, Response};

/// What a phone reads from the QR code.
#[derive(Debug, Clone)]
pub struct ScannedQr {
    pub identity_x962: Vec<u8>,
    pub secret: [u8; SECRET_LEN],
    pub domains: i64,
    pub timestamp: i64,
    pub request: String,
}

/// Decode `FIDO:/<digits>`.
pub fn scan_qr(text_in: &str) -> ScannedQr {
    let digits = text_in.strip_prefix("FIDO:/").expect("FIDO:/ prefix");
    let cbor = digits_to_bytes(digits).expect("digits");
    let value = decode_exact(&cbor).expect("cbor");
    let entries = map(&value).expect("map");
    let compressed = get_int_key(entries, 0).and_then(as_bytes).expect("key");
    let identity = PublicKey::from_sec1_bytes(compressed).expect("point");
    let secret: [u8; SECRET_LEN] = get_int_key(entries, 1)
        .and_then(as_bytes)
        .expect("secret")
        .try_into()
        .expect("16 bytes");
    ScannedQr {
        identity_x962: x962(&identity, false),
        secret,
        domains: get_int_key(entries, 2).and_then(as_i64).expect("domains"),
        timestamp: get_int_key(entries, 3).and_then(as_i64).expect("time"),
        request: get_int_key(entries, 5)
            .and_then(as_text)
            .expect("request")
            .to_string(),
    }
}

/// The phone's EID: nonce, routing id, tunnel server domain.
pub fn make_eid(routing_id: [u8; 3], domain: u16) -> Eid {
    let [lo, hi] = domain.to_le_bytes();
    let [r0, r1, r2] = routing_id;
    [0, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, r0, r1, r2, lo, hi]
}

/// The advertisement for `eid` under the QR's secret.
pub fn advert(scanned: &ScannedQr, eid: &Eid) -> Vec<u8> {
    let eid_key = qr::derive::<EID_KEY_LEN>(&scanned.secret, &[], Derived::EidKey);
    encrypt_advert(eid, &eid_key).expect("advert").to_vec()
}

/// How the phone answers.
#[derive(Debug, Clone)]
pub struct PhoneScript {
    /// 1: type bytes on every message; 0: an older phone with a padded
    /// first message and no type bytes.
    pub revision: u8,
    /// Send an update message before the answer.
    pub update_first: bool,
    /// The features in the first message (`None`: no features key).
    pub features: Option<Vec<&'static str>>,
    /// The CTAP2 status of the answer.
    pub status: u8,
    /// The answer's CBOR.
    pub answer: Value,
}

/// What the phone saw.
#[derive(Debug, Clone, Default)]
pub struct PhoneLog {
    pub path: String,
    pub subprotocol: String,
    /// The decrypted request, type byte included.
    pub request: Vec<u8>,
}

/// authData with an attested credential: rpIdHash, flags UP|UV|AT, a
/// counter, aaguid, the id's length and the id, a tiny COSE key.
pub fn auth_data_with_credential(credential_id: &[u8]) -> Vec<u8> {
    let mut data = vec![0x5a; 32];
    data.push(0x45);
    data.extend_from_slice(&[0, 0, 0, 1]);
    data.extend_from_slice(&[0x11; 16]);
    data.extend_from_slice(&u16::try_from(credential_id.len()).unwrap().to_be_bytes());
    data.extend_from_slice(credential_id);
    data.extend_from_slice(&[0xa1, 0x01, 0x02]);
    data
}

pub fn make_credential_answer(credential_id: &[u8]) -> Value {
    Value::Map(vec![
        int_entry(1, text("packed")),
        int_entry(2, bytes(&auth_data_with_credential(credential_id))),
        int_entry(3, Value::Map(vec![])),
    ])
}

pub fn get_assertion_answer(credential_id: &[u8], user: &[u8]) -> Value {
    Value::Map(vec![
        int_entry(
            1,
            Value::Map(vec![
                text_entry("id", bytes(credential_id)),
                text_entry("type", text("public-key")),
            ]),
        ),
        int_entry(2, bytes(&[0x5a; 37])),
        int_entry(3, bytes(&[0x30, 0x44, 0x02])),
        int_entry(4, Value::Map(vec![text_entry("id", bytes(user))])),
    ])
}

/// A tunnel server with the phone on the other end, for one connection.
pub struct FakePhone {
    pub addr: SocketAddr,
    pub done: JoinHandle<PhoneLog>,
}

/// Serve one tunnel on localhost as the phone that scanned `scanned` and
/// advertised `eid`.
#[expect(
    clippy::result_large_err,
    reason = "tungstenite's handshake callback returns its own ErrorResponse"
)]
pub fn serve(scanned: ScannedQr, eid: Eid, script: PhoneScript) -> FakePhone {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let done = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept");
        let seen = Arc::new(Mutex::new(PhoneLog::default()));
        let seen_in = seen.clone();
        let callback = move |request: &Request, mut response: Response| {
            let mut log = seen_in.lock().unwrap();
            log.path = request.uri().path().to_string();
            log.subprotocol = request
                .headers()
                .get("Sec-WebSocket-Protocol")
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_string();
            response.headers_mut().insert(
                "Sec-WebSocket-Protocol",
                SUBPROTOCOL.parse().expect("header"),
            );
            Ok(response)
        };
        let mut ws = tungstenite::accept_hdr(stream, callback).expect("upgrade");
        let read = |ws: &mut tungstenite::WebSocket<std::net::TcpStream>| loop {
            match ws.read().expect("read") {
                Message::Binary(data) => return data.to_vec(),
                _ => continue,
            }
        };
        let initial = read(&mut ws);
        let psk = qr::derive::<32>(&scanned.secret, &eid, Derived::Psk);
        let ephemeral = SecretKey::from_slice(&[0x44; 32]).expect("scalar");
        let (response, handshake) =
            respond(&psk, &scanned.identity_x962, &initial, &ephemeral).expect("handshake");
        ws.send(Message::Binary(response.into())).expect("send");
        let mut crypter = handshake.crypter;

        let mut post = vec![int_entry(1, bytes(&[0xa1, 0x01]))];
        if let Some(features) = &script.features {
            post.push(int_entry(
                3,
                Value::Array(features.iter().map(|f| text(f)).collect()),
            ));
        }
        let mut first = encode(&Value::Map(post)).unwrap();
        if script.revision == 0 {
            first.extend_from_slice(&[0, 0, 0]);
            first.extend_from_slice(&3u16.to_le_bytes());
        }
        ws.send(Message::Binary(crypter.encrypt(&first).unwrap().into()))
            .expect("send post");

        let sealed = read(&mut ws);
        let request = crypter.decrypt(&sealed).expect("request");
        seen.lock().unwrap().request = request;

        let typed = |kind: u8, body: Vec<u8>| {
            if script.revision >= 1 {
                let mut out = vec![kind];
                out.extend(body);
                out
            } else {
                body
            }
        };
        if script.update_first {
            let update = typed(2, vec![0xa0]);
            ws.send(Message::Binary(crypter.encrypt(&update).unwrap().into()))
                .expect("send update");
        }
        let mut payload = vec![script.status];
        if script.status == 0 {
            payload.extend(encode(&script.answer).unwrap());
        }
        let answer = typed(1, payload);
        ws.send(Message::Binary(crypter.encrypt(&answer).unwrap().into()))
            .expect("send answer");
        // Wait for Quill to close.
        let _ = ws.read();
        seen.lock().unwrap().clone()
    });
    FakePhone { addr, done }
}

/// A scanner the test drives: it says whether Bluetooth works and hands
/// over advertisements through `adverts`.
pub struct FakeScanner {
    pub available: Option<bool>,
    pub events: Arc<Mutex<Option<Sender<ScanEvent>>>>,
    pub stopped: Arc<Mutex<bool>>,
}

impl Scanner for FakeScanner {
    fn start(&mut self, events: Sender<ScanEvent>) -> bool {
        if let Some(available) = self.available {
            let _ = events.send(ScanEvent::Available(available));
        }
        *self.events.lock().unwrap() = Some(events);
        true
    }

    fn stop(&mut self) {
        *self.stopped.lock().unwrap() = true;
        *self.events.lock().unwrap() = None;
    }
}
