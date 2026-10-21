//! The caBLE v2 QR code (tdesktop `cable_core.cpp` `MakeQRKey`,
//! `EncodeQRContents`, `BytesToDigits`, `Derive`): a fresh P-256 identity
//! and a 16-byte secret per ceremony, sent to the phone as
//! `FIDO:/<digits>`, where the digits encode a CBOR map seven bytes at a
//! time.
#![deny(clippy::indexing_slicing)]

use super::crypto::{self, x962};
use crate::webauthn::ctap2::cbor::{bytes, encode, int, int_entry, text};
use crate::webauthn::ctap2::pin::random_secret;
use ciborium::Value;
use p256::SecretKey;
use rand::Rng;
use zeroize::Zeroizing;

/// The QR secret's length.
pub const SECRET_LEN: usize = 16;

/// The tunnel servers with an assigned number (tdesktop
/// `kAssignedTunnelDomains`); the QR says how many Quill knows.
pub const ASSIGNED_TUNNEL_DOMAINS: [&str; 2] = ["cable.ua5v.com", "cable.auth.com"];

/// Digits per chunk of 0..=7 bytes (tdesktop `BytesToDigits` `kWidths`).
const DIGIT_WIDTHS: [usize; 8] = [0, 3, 5, 8, 10, 13, 15, 17];

/// What the QR code carries that Quill keeps: the identity key whose
/// public half is in the code, and the shared secret.
pub struct QrKey {
    pub(super) identity: SecretKey,
    pub(super) secret: Zeroizing<[u8; SECRET_LEN]>,
}

impl QrKey {
    /// A new identity and secret (tdesktop `MakeQRKey`).
    pub fn generate(rng: &mut dyn Rng) -> Option<Self> {
        let identity = random_secret(rng)?;
        let mut secret = Zeroizing::new([0u8; SECRET_LEN]);
        rng.fill_bytes(secret.as_mut());
        Some(Self { identity, secret })
    }

    /// A key from fixed parts (tests and the fake phone).
    #[cfg(test)]
    pub fn from_parts(identity: SecretKey, secret: [u8; SECRET_LEN]) -> Self {
        Self {
            identity,
            secret: Zeroizing::new(secret),
        }
    }

    pub fn secret(&self) -> &[u8; SECRET_LEN] {
        &self.secret
    }

    /// The identity's public key, uncompressed (what the handshake hashes).
    pub fn identity_x962(&self) -> Vec<u8> {
        x962(&self.identity.public_key(), false)
    }
}

/// `FIDO:/` and the digits of `{0: compressed public key, 1: secret, 2:
/// number of assigned tunnel domains, 3: now, 4: false, 5: "mc" | "ga"}`
/// (tdesktop `EncodeQRContents`, keys in that order).
pub fn encode_contents(key: &QrKey, make_credential: bool, now_unix: i64) -> Option<String> {
    let domains = i64::try_from(ASSIGNED_TUNNEL_DOMAINS.len()).ok()?;
    let map = Value::Map(vec![
        int_entry(0, bytes(&x962(&key.identity.public_key(), true))),
        int_entry(1, bytes(key.secret.as_ref())),
        int_entry(2, int(domains)),
        int_entry(3, int(now_unix)),
        int_entry(4, Value::Bool(false)),
        int_entry(5, text(if make_credential { "mc" } else { "ga" })),
    ]);
    Some(format!("FIDO:/{}", bytes_to_digits(&encode(&map)?)))
}

/// Seven bytes at a time, little-endian, as a zero-padded decimal number
/// of 17 digits (fewer for the last, shorter chunk).
pub fn bytes_to_digits(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(7).saturating_mul(17));
    for chunk in data.chunks(7) {
        let value = chunk.iter().enumerate().fold(0u64, |value, (i, byte)| {
            value | (u64::from(*byte) << (8 * i))
        });
        let width = DIGIT_WIDTHS.get(chunk.len()).copied().unwrap_or(17);
        out.push_str(&format!("{value:0width$}"));
    }
    out
}

/// The other way (tdesktop `DigitsToBytes`); `None` for anything a QR
/// code from [`bytes_to_digits`] could not hold.
pub fn digits_to_bytes(digits: &str) -> Option<Vec<u8>> {
    fn parse(chunk: &str) -> Option<u64> {
        chunk.bytes().try_fold(0u64, |value, ch| {
            if !ch.is_ascii_digit() {
                return None;
            }
            value.checked_mul(10)?.checked_add(u64::from(ch - b'0'))
        })
    }
    if !digits.is_ascii() {
        return None;
    }
    let mut out = Vec::with_capacity(digits.len().div_ceil(17).saturating_mul(7));
    let mut rest = digits;
    while rest.len() >= 17 {
        let (chunk, tail) = rest.split_at_checked(17)?;
        let value = parse(chunk)?;
        if value >> 56 != 0 {
            return None;
        }
        out.extend_from_slice(value.to_le_bytes().get(..7)?);
        rest = tail;
    }
    if !rest.is_empty() {
        let count = DIGIT_WIDTHS.iter().position(|width| *width == rest.len())?;
        let value = parse(rest)?;
        if value >> (8 * count) != 0 {
            return None;
        }
        out.extend_from_slice(value.to_le_bytes().get(..count)?);
    }
    Some(out)
}

/// What a value derived from the QR secret is for (tdesktop
/// `DerivedValueType`); the number is HKDF's info, little-endian.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Derived {
    EidKey = 1,
    TunnelId = 2,
    Psk = 3,
}

/// HKDF-SHA-256 of the QR secret with `salt` (tdesktop `Derive`).
pub fn derive<const N: usize>(secret: &[u8], salt: &[u8], kind: Derived) -> Zeroizing<[u8; N]> {
    let mut out = Zeroizing::new([0u8; N]);
    let info = (kind as u32).to_le_bytes();
    if !crypto::hkdf_sha256(out.as_mut(), secret, salt, &info) {
        // Only for N > 8160, which no caller asks for; zeros as tdesktop.
        out.as_mut().fill(0);
    }
    out
}
