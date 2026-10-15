//! A scripted security key for tests: it speaks CTAPHID over fake HID
//! reports and answers getInfo, clientPIN (both PIN protocols, with a real
//! P-256 key agreement), makeCredential and getAssertion the way CTAP 2.1
//! describes. The messages follow the CTAP 2.1 specification's structures;
//! they are not captured from a physical key.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test-only fake"
)]

use super::cbor::{
    as_bytes, as_i64, bytes, get_int_key, get_text_key, int, int_entry, map, text, text_entry,
};
use super::ctaphid::{
    Assembly, BROADCAST_CID, CMD_CANCEL, CMD_CBOR, CMD_INIT, CMD_KEEPALIVE, HidDevice, HidError,
    KEEPALIVE_UP_NEEDED, frame,
};
use super::pin::{Protocol, agree, cose_key, public_from_cose};
use super::{command, status};
use ciborium::Value;
use p256::SecretKey;
use sha2_next::{Digest, Sha256};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const CID: u32 = 0x0102_0304;
pub const TOKEN: [u8; 32] = [0x5A; 32];
pub const CREDENTIAL_ID: [u8; 20] = [0xC1; 20];
pub const AAGUID: [u8; 16] = [0xAA; 16];

/// How the key behaves; shared with the test through `Arc<Mutex<_>>`.
pub struct Script {
    pub versions: Vec<&'static str>,
    pub pin_protocols: Vec<i64>,
    pub pin_uv_auth_token: bool,
    /// The PIN set on the key, if any.
    pub pin: Option<String>,
    pub pin_retries: u8,
    /// Keepalives (user presence needed) before each answer.
    pub keepalives: usize,
    /// Keep sending keepalives until cancelled.
    pub hold: bool,
    /// Answer the next makeCredential / getAssertion with this status.
    pub fail_next: Option<u8>,
    /// Requests seen: command byte and decoded parameters.
    pub seen: Vec<(u8, Value)>,
    pub cancels: usize,
}

impl Default for Script {
    fn default() -> Self {
        Self {
            versions: vec!["U2F_V2", "FIDO_2_0", "FIDO_2_1"],
            pin_protocols: vec![2, 1],
            pin_uv_auth_token: true,
            pin: None,
            pin_retries: 8,
            keepalives: 2,
            hold: false,
            fail_next: None,
            seen: Vec::new(),
            cancels: 0,
        }
    }
}

pub struct FakeKey {
    script: Arc<Mutex<Script>>,
    out: VecDeque<Vec<u8>>,
    assembly: Assembly,
    secret: SecretKey,
    shared: Option<super::pin::SharedSecret>,
    holding: bool,
}

impl FakeKey {
    pub fn new(script: Arc<Mutex<Script>>) -> Self {
        Self {
            script,
            out: VecDeque::new(),
            assembly: Assembly::default(),
            secret: SecretKey::from_slice(&[7u8; 32]).unwrap(),
            shared: None,
            holding: false,
        }
    }

    fn reply(&mut self, cid: u32, cmd: u8, payload: &[u8]) {
        for report in frame(cid, cmd, payload, 64).unwrap() {
            self.out.push_back(report);
        }
    }

    fn cbor_reply(&mut self, code: u8, body: Option<Value>) {
        let mut payload = vec![code];
        if let Some(body) = body {
            ciborium::into_writer(&body, &mut payload).unwrap();
        }
        self.reply(CID, CMD_CBOR, &payload);
    }

    fn info(&self) -> Value {
        let script = self.script.lock().unwrap();
        let mut options = vec![
            text_entry("rk", Value::Bool(true)),
            text_entry("up", Value::Bool(true)),
            text_entry("plat", Value::Bool(false)),
            text_entry("clientPin", Value::Bool(script.pin.is_some())),
        ];
        if script.pin_uv_auth_token {
            options.push(text_entry("pinUvAuthToken", Value::Bool(true)));
        }
        Value::Map(vec![
            int_entry(
                1,
                Value::Array(script.versions.iter().map(|v| text(v)).collect()),
            ),
            int_entry(3, bytes(&AAGUID)),
            int_entry(4, Value::Map(options)),
            int_entry(5, int(1200)),
            int_entry(
                6,
                Value::Array(script.pin_protocols.iter().map(|p| int(*p)).collect()),
            ),
        ])
    }

    fn protocol(params: &[(Value, Value)]) -> Protocol {
        match get_int_key(params, 1).and_then(as_i64) {
            Some(2) => Protocol::Two,
            _ => Protocol::One,
        }
    }

    fn client_pin(&mut self, params: &[(Value, Value)]) {
        let protocol = Self::protocol(params);
        match get_int_key(params, 2).and_then(as_i64) {
            Some(2) => {
                let key = cose_key(&self.secret.public_key());
                self.cbor_reply(status::OK, Some(Value::Map(vec![int_entry(1, key)])));
            }
            Some(5 | 9) => {
                let platform = get_int_key(params, 3).and_then(public_from_cose).unwrap();
                let shared = agree(protocol, &self.secret, &platform);
                let pin_hash = shared
                    .decrypt(get_int_key(params, 6).and_then(as_bytes).unwrap())
                    .unwrap();
                let mut script = self.script.lock().unwrap();
                if script.pin_retries == 0 {
                    drop(script);
                    return self.cbor_reply(status::PIN_BLOCKED, None);
                }
                let expected = Sha256::digest(script.pin.clone().unwrap_or_default().as_bytes());
                if pin_hash.as_slice() != &expected[..16] {
                    script.pin_retries -= 1;
                    let code = if script.pin_retries == 0 {
                        status::PIN_BLOCKED
                    } else {
                        status::PIN_INVALID
                    };
                    drop(script);
                    return self.cbor_reply(code, None);
                }
                drop(script);
                let mut rng = rand::rngs::OsRng;
                let token = shared.encrypt(&TOKEN, &mut rng).unwrap();
                self.shared = Some(shared);
                self.cbor_reply(
                    status::OK,
                    Some(Value::Map(vec![int_entry(2, bytes(&token))])),
                );
            }
            _ => self.cbor_reply(status::INVALID_COMMAND, None),
        }
    }

    /// The status a makeCredential / getAssertion gets before its answer.
    fn gate(&mut self, params: &[(Value, Value)], hash_key: i64, param_key: i64) -> Option<u8> {
        let mut script = self.script.lock().unwrap();
        if let Some(code) = script.fail_next.take() {
            return Some(code);
        }
        let has_pin = script.pin.is_some();
        drop(script);
        match get_int_key(params, param_key).and_then(as_bytes) {
            None if has_pin => Some(status::PIN_REQUIRED),
            None => None,
            Some(param) => {
                let hash = get_int_key(params, hash_key).and_then(as_bytes).unwrap();
                let protocol = Self::protocol(&[(
                    int(1),
                    get_int_key(params, param_key + 1)
                        .cloned()
                        .unwrap_or(int(1)),
                )]);
                (protocol.authenticate(&TOKEN, hash) != param).then_some(status::PIN_AUTH_INVALID)
            }
        }
    }

    fn auth_data(rp_id: &str, with_credential: bool) -> Vec<u8> {
        let mut data = Sha256::digest(rp_id.as_bytes()).to_vec();
        data.push(if with_credential { 0x45 } else { 0x05 });
        data.extend_from_slice(&[0, 0, 0, 1]);
        if with_credential {
            data.extend_from_slice(&AAGUID);
            data.extend_from_slice(&(CREDENTIAL_ID.len() as u16).to_be_bytes());
            data.extend_from_slice(&CREDENTIAL_ID);
            data.extend_from_slice(&[0xA5, 0x01, 0x02, 0x03, 0x26]);
        }
        data
    }

    fn presence(&mut self) {
        let count = self.script.lock().unwrap().keepalives;
        for _ in 0..count {
            self.reply(CID, CMD_KEEPALIVE, &[KEEPALIVE_UP_NEEDED]);
        }
    }

    fn command(&mut self, payload: &[u8]) {
        let Some((&cmd, rest)) = payload.split_first() else {
            return;
        };
        let params: Value = if rest.is_empty() {
            Value::Map(Vec::new())
        } else {
            ciborium::from_reader(rest).unwrap()
        };
        self.script.lock().unwrap().seen.push((cmd, params.clone()));
        let params = map(&params).unwrap_or_default().to_vec();
        match cmd {
            command::GET_INFO => {
                let info = self.info();
                self.cbor_reply(status::OK, Some(info));
            }
            command::CLIENT_PIN => self.client_pin(&params),
            command::MAKE_CREDENTIAL => {
                if let Some(code) = self.gate(&params, 1, 8) {
                    return self.cbor_reply(code, None);
                }
                if self.script.lock().unwrap().hold {
                    self.holding = true;
                    return;
                }
                self.presence();
                let rp = get_int_key(&params, 2).and_then(map).unwrap();
                let rp_id = get_text_key(rp, "id")
                    .and_then(super::cbor::as_text)
                    .unwrap();
                let answer = Value::Map(vec![
                    int_entry(1, text("packed")),
                    int_entry(2, bytes(&Self::auth_data(rp_id, true))),
                    int_entry(3, Value::Map(vec![text_entry("alg", int(-7))])),
                ]);
                self.cbor_reply(status::OK, Some(answer));
            }
            command::GET_ASSERTION => {
                if let Some(code) = self.gate(&params, 2, 6) {
                    return self.cbor_reply(code, None);
                }
                self.presence();
                let rp_id = get_int_key(&params, 1)
                    .and_then(super::cbor::as_text)
                    .unwrap()
                    .to_string();
                let answer = Value::Map(vec![
                    int_entry(
                        1,
                        Value::Map(vec![
                            text_entry("id", bytes(&CREDENTIAL_ID)),
                            text_entry("type", text("public-key")),
                        ]),
                    ),
                    int_entry(2, bytes(&Self::auth_data(&rp_id, false))),
                    int_entry(3, bytes(&[0x30, 0x44, 0x02, 0x20])),
                    int_entry(4, Value::Map(vec![text_entry("id", bytes(&[1, 2, 3, 4]))])),
                    int_entry(5, int(2)),
                ]);
                self.cbor_reply(status::OK, Some(answer));
            }
            _ => self.cbor_reply(status::INVALID_COMMAND, None),
        }
    }
}

impl HidDevice for FakeKey {
    fn report_len(&self) -> usize {
        64
    }

    fn write_report(&mut self, report: &[u8]) -> Result<(), HidError> {
        assert_eq!(report.len(), 64, "reports are fixed size");
        let cid = u32::from_be_bytes(report[..4].try_into().unwrap());
        let Some((cmd, payload)) = self.assembly.feed(cid, report)? else {
            return Ok(());
        };
        match cmd {
            CMD_INIT => {
                assert_eq!(cid, BROADCAST_CID);
                let mut answer = payload.clone();
                answer.extend_from_slice(&CID.to_be_bytes());
                answer.extend_from_slice(&[2, 5, 4, 3, 0x05]);
                self.reply(BROADCAST_CID, CMD_INIT, &answer);
            }
            CMD_CBOR => {
                assert_eq!(cid, CID);
                self.command(&payload);
            }
            CMD_CANCEL => {
                self.script.lock().unwrap().cancels += 1;
                if self.holding {
                    self.holding = false;
                    self.out.clear();
                    self.cbor_reply(status::KEEPALIVE_CANCEL, None);
                }
            }
            other => panic!("unexpected CTAPHID command {other:#x}"),
        }
        Ok(())
    }

    fn read_report(&mut self, _timeout: Duration) -> Result<Option<Vec<u8>>, HidError> {
        if let Some(report) = self.out.pop_front() {
            return Ok(Some(report));
        }
        std::thread::sleep(Duration::from_millis(2));
        if self.holding {
            let mut keepalive = frame(CID, CMD_KEEPALIVE, &[KEEPALIVE_UP_NEEDED], 64).unwrap();
            return Ok(keepalive.pop());
        }
        Ok(None)
    }
}

/// A device source that hands out fresh fake keys for one script, after
/// `absent` empty looks.
pub struct FakeSource {
    pub script: Arc<Mutex<Script>>,
    pub absent: usize,
    pub looks: Arc<Mutex<usize>>,
}

impl crate::webauthn::DeviceSource for FakeSource {
    fn open_all(&mut self) -> Vec<Box<dyn HidDevice>> {
        *self.looks.lock().unwrap() += 1;
        if self.absent > 0 {
            self.absent -= 1;
            return Vec::new();
        }
        vec![Box::new(FakeKey::new(self.script.clone()))]
    }
}
