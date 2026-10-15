//! The phone's Bluetooth advertisement (tdesktop `cable_core.cpp`
//! `DecryptAdvert`, `ToEidComponents`, `DecodeTunnelServerDomain`): 16
//! bytes of AES-256 encrypted EID and a 4-byte HMAC tag, both keyed by
//! the EID key derived from the QR secret. The EID says which tunnel
//! server the phone went to and under which routing id.
#![deny(clippy::indexing_slicing)]

use super::crypto::{aes256_decrypt_block, hmac_sha256, sha256};
use super::qr::ASSIGNED_TUNNEL_DOMAINS;

/// The service data of a caBLE advertisement.
pub const ADVERT_LEN: usize = 20;
/// The EID key: AES-256 key, then HMAC key.
pub const EID_KEY_LEN: usize = 64;

/// The decrypted EID: a zero byte, the nonce (10), the routing id (3) and
/// the tunnel server domain (2, little-endian).
pub type Eid = [u8; 16];

/// What the EID says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EidParts {
    pub nonce: [u8; 10],
    pub routing_id: [u8; 3],
    pub tunnel_server_domain: u16,
}

/// The EID inside `service_data`, when it was made with this ceremony's
/// key and names a tunnel server Quill can reach.
pub fn decrypt_advert(service_data: &[u8], eid_key: &[u8; EID_KEY_LEN]) -> Option<Eid> {
    let body: &[u8; 16] = service_data.get(..16)?.try_into().ok()?;
    let tag = service_data.get(16..ADVERT_LEN)?;
    let (aes_key, hmac_key) = eid_key.split_at(32);
    let expected = hmac_sha256(hmac_key, body);
    if expected.get(..4) != Some(tag) {
        return None;
    }
    let plain = aes256_decrypt_block(aes_key, body)?;
    let [reserved, .., lo, hi] = plain;
    if reserved != 0 {
        return None;
    }
    let domain = u16::from_le_bytes([lo, hi]);
    let assigned = usize::from(domain) < ASSIGNED_TUNNEL_DOMAINS.len();
    (domain >= 256 || assigned).then_some(plain)
}

/// The parts of a decrypted EID.
pub fn parts(eid: &Eid) -> EidParts {
    let [
        _,
        n0,
        n1,
        n2,
        n3,
        n4,
        n5,
        n6,
        n7,
        n8,
        n9,
        r0,
        r1,
        r2,
        lo,
        hi,
    ] = *eid;
    EidParts {
        nonce: [n0, n1, n2, n3, n4, n5, n6, n7, n8, n9],
        routing_id: [r0, r1, r2],
        tunnel_server_domain: u16::from_le_bytes([lo, hi]),
    }
}

/// The tunnel server's host name: an assigned one below 256, else
/// `cable.` and 13 base32 characters from a hash of the number, and one of
/// four top-level domains.
pub fn tunnel_server_domain(domain: u16) -> Option<String> {
    if domain < 256 {
        return ASSIGNED_TUNNEL_DOMAINS
            .get(usize::from(domain))
            .map(|name| (*name).to_string());
    }
    const BASE32: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    const TLDS: [&str; 4] = [".com", ".org", ".net", ".info"];
    let mut input = b"caBLEv2 tunnel server domain".to_vec();
    input.extend_from_slice(&domain.to_le_bytes());
    input.push(0);
    let digest = sha256(&input);
    let mut value = u64::from_le_bytes(digest.get(..8)?.try_into().ok()?);
    let tld = TLDS.get(usize::try_from(value & 3).ok()?)?;
    value >>= 2;
    let mut out = String::from("cable.");
    while value != 0 {
        out.push(char::from(*BASE32.get(usize::try_from(value & 31).ok()?)?));
        value >>= 5;
    }
    out.push_str(tld);
    Some(out)
}

/// Lowercase hex (the tunnel URL's routing and tunnel ids).
pub fn hex_lower(data: &[u8]) -> String {
    data.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// An advertisement the way a phone makes it (tests and the fake phone).
#[cfg(test)]
pub fn encrypt_advert(eid: &Eid, eid_key: &[u8; EID_KEY_LEN]) -> Option<[u8; ADVERT_LEN]> {
    let (aes_key, hmac_key) = eid_key.split_at(32);
    let body = super::crypto::aes256_encrypt_block(aes_key, eid)?;
    let tag = hmac_sha256(hmac_key, &body);
    let mut out = [0u8; ADVERT_LEN];
    for (slot, byte) in out.iter_mut().zip(body.iter().chain(tag.iter())) {
        *slot = *byte;
    }
    Some(out)
}
