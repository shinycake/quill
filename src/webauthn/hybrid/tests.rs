//! caBLE v2 against Telegram Desktop's own output
//! (tests/fixtures/cable/tdesktop-vectors.txt), and the parts on their own.

use super::crypto::x962;
use super::eid::{self, EID_KEY_LEN};
use super::messages::{
    self, MessageType, PostHandshake, Reply, classify_reply, ctap_answer, frame_request,
    parse_post_handshake,
};
use super::noise::{Crypter, Initiator, respond};
use super::qr::{self, Derived, QrKey, bytes_to_digits, digits_to_bytes};
use crate::webauthn::ctap2::cbor::{bytes, encode, int, int_entry, text};
use crate::webauthn::{CreationOptions, RequestOptions};
use ciborium::Value;
use p256::SecretKey;
use zeroize::Zeroizing;

const VECTORS: &str = include_str!("../../../tests/fixtures/cable/tdesktop-vectors.txt");

fn vector(name: &str) -> &'static str {
    VECTORS
        .lines()
        .filter(|line| !line.starts_with('#'))
        .find_map(|line| {
            let (key, value) = line.split_once(": ")?;
            (key == name).then_some(value)
        })
        .unwrap_or_else(|| panic!("no vector {name}"))
}

fn unhex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn hex(name: &str) -> Vec<u8> {
    unhex(vector(name))
}

fn fixed_qr() -> QrKey {
    let mut secret = [0u8; 16];
    for (i, byte) in secret.iter_mut().enumerate() {
        *byte = i as u8 + 1;
    }
    QrKey::from_parts(SecretKey::from_slice(&[0x11; 32]).unwrap(), secret)
}

fn eid_key() -> Zeroizing<[u8; EID_KEY_LEN]> {
    qr::derive::<EID_KEY_LEN>(fixed_qr().secret(), &[], Derived::EidKey)
}

#[test]
fn digits_match_tdesktop() {
    assert_eq!(bytes_to_digits(&hex("digits_in")), vector("digits_out"));
    assert_eq!(
        bytes_to_digits(&hex("digits_long_in")),
        vector("digits_long_out")
    );
    assert_eq!(
        digits_to_bytes(vector("digits_long_out")).unwrap(),
        hex("digits_long_in")
    );
    assert_eq!(bytes_to_digits(&[]), "");
    assert_eq!(bytes_to_digits(&[255]), "255");
}

#[test]
fn digits_round_trip_every_length() {
    for len in 0..40usize {
        let data: Vec<u8> = (0..len).map(|i| (i * 37 + 255) as u8).collect();
        assert_eq!(digits_to_bytes(&bytes_to_digits(&data)).unwrap(), data);
    }
}

#[test]
fn digits_reject_what_no_qr_holds() {
    // A length no chunk has, a letter, a chunk too large for its bytes.
    assert_eq!(digits_to_bytes("12"), None);
    assert_eq!(digits_to_bytes("12a"), None);
    assert_eq!(digits_to_bytes("256"), None);
    assert_eq!(digits_to_bytes("99999999999999999"), None);
    assert_eq!(digits_to_bytes("٣٣٣"), None);
}

#[test]
fn qr_contents_match_tdesktop() {
    let key = fixed_qr();
    assert_eq!(
        qr::encode_contents(&key, true, 1_760_000_000).unwrap(),
        vector("qr_mc")
    );
    assert_eq!(
        qr::encode_contents(&key, false, 1_760_000_000).unwrap(),
        vector("qr_ga")
    );
}

#[test]
fn derived_values_match_tdesktop() {
    assert_eq!(eid_key().to_vec(), hex("eid_derived"));
    let tunnel = qr::derive::<16>(fixed_qr().secret(), &[], Derived::TunnelId);
    assert_eq!(tunnel.to_vec(), hex("tunnel_id"));
}

#[test]
fn adverts_decrypt_like_tdesktop() {
    let key = eid_key();
    let eid = eid::decrypt_advert(&hex("advert_domain0"), &key).unwrap();
    let parts = eid::parts(&eid);
    assert_eq!(
        parts.nonce,
        [0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9]
    );
    assert_eq!(parts.routing_id, [1, 2, 3]);
    assert_eq!(parts.tunnel_server_domain, 0);
    let psk = qr::derive::<32>(fixed_qr().secret(), &eid, Derived::Psk);
    assert_eq!(psk.to_vec(), hex("psk_domain0"));

    // A hashed domain is fine; an unassigned small one is not.
    let hashed = eid::decrypt_advert(&hex("advert_domain300"), &key).unwrap();
    assert_eq!(eid::parts(&hashed).tunnel_server_domain, 300);
    assert_eq!(eid::decrypt_advert(&hex("advert_domain5"), &key), None);

    // A broken tag, a short advert, someone else's key.
    let mut tampered = hex("advert_domain0");
    tampered[19] ^= 1;
    assert_eq!(eid::decrypt_advert(&tampered, &key), None);
    assert_eq!(
        eid::decrypt_advert(&hex("advert_domain0")[..19], &key),
        None
    );
    let other = qr::derive::<EID_KEY_LEN>(&[9; 16], &[], Derived::EidKey);
    assert_eq!(eid::decrypt_advert(&hex("advert_domain0"), &other), None);
    // Extra bytes after the 20 are ignored, as tdesktop does.
    let mut longer = hex("advert_domain0");
    longer.push(0);
    assert!(eid::decrypt_advert(&longer, &key).is_some());
}

#[test]
fn tunnel_domains_match_tdesktop() {
    for domain in [0u16, 1, 256, 300, 1000, 65535] {
        assert_eq!(
            eid::tunnel_server_domain(domain).unwrap(),
            vector(&format!("domain_{domain}")),
            "domain {domain}"
        );
    }
    assert_eq!(eid::tunnel_server_domain(2), None);
    assert_eq!(eid::hex_lower(&[0, 0xab, 0x10]), "00ab10");
}

fn creation() -> CreationOptions {
    CreationOptions {
        rp_id: "telegram.org".into(),
        rp_name: "Telegram".into(),
        user_id: vec![1, 2, 3, 4],
        user_name: "name".into(),
        user_display_name: "Display".into(),
        challenge: vec![0xde, 0xad],
        algorithms: vec![-7, -257],
        timeout_ms: 60_000,
    }
}

fn request(allow: Vec<Vec<u8>>) -> RequestOptions {
    RequestOptions {
        rp_id: "telegram.org".into(),
        challenge: vec![0xbe, 0xef],
        allow_credentials: allow,
        ..Default::default()
    }
}

#[test]
fn ctap_requests_match_tdesktop() {
    let hash = [0x33; 32];
    assert_eq!(
        messages::make_credential_request(&hash, &creation()).unwrap(),
        hex("make_credential")
    );
    assert_eq!(
        messages::get_assertion_request(&hash, &request(vec![vec![9, 9, 9]])).unwrap(),
        hex("get_assertion")
    );
    assert_eq!(
        messages::get_assertion_request(&hash, &request(vec![])).unwrap(),
        hex("get_assertion_no_allow")
    );
    // No algorithm offered: ES256 (tdesktop `RegisterViaCable`).
    let mut none = creation();
    none.algorithms.clear();
    let request = messages::make_credential_request(&hash, &none).unwrap();
    let value = messages::decode_exact(&request[1..]).unwrap();
    let entries = crate::webauthn::ctap2::cbor::map(&value).unwrap();
    let params = crate::webauthn::ctap2::cbor::get_int_key(entries, 4).unwrap();
    assert_eq!(
        params,
        &Value::Array(vec![Value::Map(vec![
            (text("alg"), int(-7)),
            (text("type"), text("public-key")),
        ])])
    );
}

#[test]
fn handshake_matches_tdesktop() {
    let qr = fixed_qr();
    let psk: [u8; 32] = hex("psk_domain0").try_into().unwrap();
    let mut initiator = Initiator::new(&psk, &qr.identity);
    let ephemeral = SecretKey::from_slice(&[0x22; 32]).unwrap();
    let first = initiator.initial_message(ephemeral).unwrap();
    assert_eq!(first, hex("init_msg"));
    let handshake = initiator.process_response(&hex("response")).unwrap();
    assert_eq!(handshake.hash.to_vec(), hex("handshake_hash"));
    let mut crypter = handshake.crypter;
    assert_eq!(
        crypter.decrypt(&hex("post_cipher")).unwrap(),
        hex("post_plain")
    );
    let out = frame_request(&hex("make_credential"), 1);
    assert_eq!(crypter.encrypt(&out).unwrap(), hex("ctap_out_cipher"));
}

#[test]
fn handshake_rejects_a_wrong_response() {
    let qr = fixed_qr();
    let psk: [u8; 32] = hex("psk_domain0").try_into().unwrap();
    let mut initiator = Initiator::new(&psk, &qr.identity);
    initiator
        .initial_message(SecretKey::from_slice(&[0x22; 32]).unwrap())
        .unwrap();
    let mut response = hex("response");
    response[70] ^= 1;
    assert!(initiator.process_response(&response).is_none());

    let initiator = Initiator::new(&psk, &qr.identity);
    // No first message yet, and a short response.
    assert!(initiator.process_response(&hex("response")).is_none());
    let mut initiator = Initiator::new(&psk, &qr.identity);
    initiator
        .initial_message(SecretKey::from_slice(&[0x22; 32]).unwrap())
        .unwrap();
    assert!(initiator.process_response(&hex("response")[..80]).is_none());
}

#[test]
fn both_sides_of_the_handshake_agree() {
    let qr = fixed_qr();
    let psk = [7u8; 32];
    let mut initiator = Initiator::new(&psk, &qr.identity);
    let first = initiator
        .initial_message(SecretKey::from_slice(&[0x22; 32]).unwrap())
        .unwrap();
    let phone_key = SecretKey::from_slice(&[0x44; 32]).unwrap();
    let (response, phone) = respond(&psk, &qr.identity_x962(), &first, &phone_key).unwrap();
    let ours = initiator.process_response(&response).unwrap();
    assert_eq!(ours.hash, phone.hash);
    let (mut ours, mut phone) = (ours.crypter, phone.crypter);
    for len in [0usize, 1, 30, 31, 32, 100] {
        let message = vec![0x5a; len];
        let sealed = ours.encrypt(&message).unwrap();
        // Padded to a multiple of 32, plus the tag.
        assert_eq!((sealed.len() - 16) % 32, 0);
        assert!(sealed.len() - 16 > len);
        assert_eq!(phone.decrypt(&sealed).unwrap(), message);
        let back = phone.encrypt(&message).unwrap();
        assert_eq!(ours.decrypt(&back).unwrap(), message);
    }
    // Out of order: the sequence numbers no longer line up.
    let a = ours.encrypt(b"a").unwrap();
    let b = ours.encrypt(b"b").unwrap();
    assert!(phone.decrypt(&b).is_none());
    // A failed open does not advance the counter.
    assert_eq!(phone.decrypt(&a).unwrap(), b"a");

    // The phone with another PSK cannot answer.
    let mut initiator = Initiator::new(&psk, &qr.identity);
    let first = initiator
        .initial_message(SecretKey::from_slice(&[0x22; 32]).unwrap())
        .unwrap();
    assert!(respond(&[8; 32], &qr.identity_x962(), &first, &phone_key).is_none());
}

#[test]
fn crypter_rejects_bad_padding() {
    let mut writer = Crypter::new(Zeroizing::new([1; 32]), Zeroizing::new([2; 32]));
    let mut reader = Crypter::new(Zeroizing::new([2; 32]), Zeroizing::new([1; 32]));
    // A message whose padding byte claims more than there is.
    let sealed = super::crypto::seal(&[2; 32], &[0; 12], &[1, 2, 200], &[]).unwrap();
    assert!(reader.decrypt(&sealed).is_none());
    let sealed = writer.encrypt(b"x").unwrap();
    assert!(reader.decrypt(&sealed[..10]).is_none());
}

#[test]
fn post_handshake_messages() {
    let plain = hex("post_plain");
    assert_eq!(
        parse_post_handshake(&plain).unwrap(),
        PostHandshake {
            get_info: vec![0xa1],
            protocol_revision: 1,
            supports_ctap: true,
        }
    );
    // Older phones pad: a two-byte count, or a one-byte one.
    let mut padded = plain.clone();
    padded.extend_from_slice(&[0, 0, 2, 0]);
    assert_eq!(parse_post_handshake(&padded).unwrap().protocol_revision, 0);
    let mut padded = plain.clone();
    padded.extend_from_slice(&[0, 1]);
    assert_eq!(parse_post_handshake(&padded).unwrap().protocol_revision, 0);
    // Features without "ctap"; no features at all means ctap.
    let no_ctap = encode(&Value::Map(vec![int_entry(
        3,
        Value::Array(vec![text("dc")]),
    )]))
    .unwrap();
    assert!(!parse_post_handshake(&no_ctap).unwrap().supports_ctap);
    let bare = encode(&Value::Map(vec![])).unwrap();
    assert!(parse_post_handshake(&bare).unwrap().supports_ctap);
    // Wrong types, not a map, nothing.
    let wrong = encode(&Value::Map(vec![int_entry(1, text("info"))])).unwrap();
    assert!(parse_post_handshake(&wrong).is_none());
    assert!(parse_post_handshake(&encode(&int(5)).unwrap()).is_none());
    assert!(parse_post_handshake(&[]).is_none());
    assert!(parse_post_handshake(&[0xff, 0xff, 0xff]).is_none());
}

#[test]
fn replies_by_type() {
    assert_eq!(classify_reply(vec![2, 0xa0], 1), Reply::Update);
    assert_eq!(
        classify_reply(vec![1, 0, 0xa0], 1),
        Reply::Ctap(vec![0, 0xa0])
    );
    assert_eq!(classify_reply(vec![0], 1), Reply::Unexpected);
    assert_eq!(classify_reply(vec![9, 1], 1), Reply::Unexpected);
    assert_eq!(classify_reply(vec![], 1), Reply::Unexpected);
    assert_eq!(classify_reply(vec![0, 0xa0], 0), Reply::Ctap(vec![0, 0xa0]));
    assert_eq!(ctap_answer(&[0, 0xa0]), Ok(Value::Map(vec![])));
    assert_eq!(ctap_answer(&[0x27]), Err(Some(0x27)));
    assert_eq!(ctap_answer(&[]), Err(None));
    assert_eq!(ctap_answer(&[0, 0xa1]), Err(None));
    assert_eq!(frame_request(&[1, 2], 0), vec![1, 2]);
    assert_eq!(
        frame_request(&[1, 2], 1),
        vec![MessageType::Ctap as u8, 1, 2]
    );
}

#[test]
fn deep_cbor_is_refused() {
    let mut nested = Value::Map(vec![]);
    for _ in 0..40 {
        nested = Value::Array(vec![nested]);
    }
    assert!(messages::decode_exact(&encode(&nested).unwrap()).is_none());
    assert!(messages::decode_exact(&encode(&bytes(&[1])).unwrap()).is_some());
}

#[test]
fn cable_uuids() {
    use super::ble::is_cable_uuid;
    assert!(is_cable_uuid("0000fde2-0000-1000-8000-00805f9b34fb"));
    assert!(is_cable_uuid("0000FFF9-0000-1000-8000-00805F9B34FB"));
    assert!(!is_cable_uuid("0000180f-0000-1000-8000-00805f9b34fb"));
}

#[test]
fn the_qr_key_shows_its_public_half() {
    let qr = fixed_qr();
    assert_eq!(qr.identity_x962(), x962(&qr.identity.public_key(), false));
    assert_eq!(qr.identity_x962().len(), 65);
}

#[path = "tests_flow.rs"]
mod flow;
