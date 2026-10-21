//! PIN/UV auth protocols one and two (CTAP 2.1 sections 6.5.6 and 6.5.7)
//! and the `authenticatorClientPIN` subcommands that turn a PIN into a
//! token: `getKeyAgreement`, `getPinToken` and, on 2.1 keys,
//! `getPinUvAuthTokenUsingPinWithPermissions`. Which protocol and which
//! subcommand follow libfido2, which Telegram Desktop uses: protocol two
//! when the key lists it, and permissions whenever the key supports them.
#![deny(clippy::indexing_slicing)]

use super::cbor::{as_bytes, bytes, get_int_key, int, int_entry, map};
use super::{Authenticator, Control, CtapError, command};
use aes::Aes256;
use cbc::cipher::block_padding::NoPadding;
use cbc::cipher::{BlockModeDecrypt, BlockModeEncrypt, KeyIvInit};
use ciborium::Value;
use hkdf::Hkdf;
use hmac::{Hmac, KeyInit, Mac};
use p256::elliptic_curve::sec1::ToSec1Point;
use p256::{PublicKey, SecretKey};
use rand::Rng;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

/// `authenticatorClientPIN` subcommands.
const SUB_GET_KEY_AGREEMENT: i64 = 0x02;
const SUB_GET_PIN_TOKEN: i64 = 0x05;
const SUB_GET_TOKEN_WITH_PERMISSIONS: i64 = 0x09;

/// Token permissions (CTAP 2.1 section 6.5.5.7).
pub const PERMISSION_MAKE_CREDENTIAL: u8 = 0x01;
pub const PERMISSION_GET_ASSERTION: u8 = 0x02;

/// COSE `ECDH-ES + HKDF-256`, the algorithm of every keyAgreement key.
const COSE_ECDH_ES_HKDF_256: i64 = -25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    One,
    Two,
}

impl Protocol {
    /// libfido2 `fido_dev_get_pin_protocol`: two when listed, else one
    /// (2.0 keys list `[1]` or nothing).
    pub fn choose(listed: &[i64]) -> Self {
        if listed.contains(&2) {
            Self::Two
        } else {
            Self::One
        }
    }

    pub fn number(self) -> i64 {
        match self {
            Self::One => 1,
            Self::Two => 2,
        }
    }

    /// `authenticate(key, message)`: HMAC-SHA-256, cut to 16 bytes in
    /// protocol one.
    pub fn authenticate(self, key: &[u8], message: &[u8]) -> Vec<u8> {
        let Ok(mut mac) = <Hmac<Sha256> as KeyInit>::new_from_slice(key) else {
            return Vec::new();
        };
        mac.update(message);
        let full = mac.finalize().into_bytes().to_vec();
        match self {
            Self::One => full.get(..16).map(<[u8]>::to_vec).unwrap_or_default(),
            Self::Two => full,
        }
    }
}

/// The secret both sides derived from ECDH.
pub struct SharedSecret {
    protocol: Protocol,
    /// One: the AES and HMAC key. Two: the HMAC key.
    hmac_key: Zeroizing<Vec<u8>>,
    /// One: same as `hmac_key`. Two: the AES key.
    aes_key: Zeroizing<Vec<u8>>,
}

fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

impl SharedSecret {
    /// Derive from the ECDH x coordinate `z` (protocol one: SHA-256;
    /// two: HKDF-SHA-256 with a zero salt and the two CTAP labels).
    pub fn derive(protocol: Protocol, z: &[u8]) -> Self {
        match protocol {
            Protocol::One => {
                let key = Zeroizing::new(sha256(z).to_vec());
                Self {
                    protocol,
                    hmac_key: key.clone(),
                    aes_key: key,
                }
            }
            Protocol::Two => {
                let hkdf = Hkdf::<Sha256>::new(Some(&[0u8; 32]), z);
                let mut hmac_key = Zeroizing::new(vec![0u8; 32]);
                let mut aes_key = Zeroizing::new(vec![0u8; 32]);
                // 32 bytes is always a valid HKDF-SHA-256 output length.
                let _ = hkdf.expand(b"CTAP2 HMAC key", &mut hmac_key);
                let _ = hkdf.expand(b"CTAP2 AES key", &mut aes_key);
                Self {
                    protocol,
                    hmac_key,
                    aes_key,
                }
            }
        }
    }

    pub fn protocol(&self) -> Protocol {
        self.protocol
    }

    /// AES-256-CBC without padding: a zero IV in protocol one, a random IV
    /// sent in front of the ciphertext in protocol two.
    pub fn encrypt(&self, plain: &[u8], rng: &mut dyn Rng) -> Option<Vec<u8>> {
        if !plain.len().is_multiple_of(16) {
            return None;
        }
        let mut iv = [0u8; 16];
        if self.protocol == Protocol::Two {
            rng.fill_bytes(&mut iv);
        }
        let mut buf = plain.to_vec();
        let encryptor = cbc::Encryptor::<Aes256>::new_from_slices(&self.aes_key, &iv).ok()?;
        encryptor
            .encrypt_padded::<NoPadding>(&mut buf, plain.len())
            .ok()?;
        Some(match self.protocol {
            Protocol::One => buf,
            Protocol::Two => iv.iter().copied().chain(buf).collect(),
        })
    }

    pub fn decrypt(&self, data: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
        let (iv, cipher) = match self.protocol {
            Protocol::One => ([0u8; 16], data),
            Protocol::Two => {
                let (iv, rest) = data.split_first_chunk::<16>()?;
                (*iv, rest)
            }
        };
        if cipher.is_empty() || !cipher.len().is_multiple_of(16) {
            return None;
        }
        let mut buf = Zeroizing::new(cipher.to_vec());
        let decryptor = cbc::Decryptor::<Aes256>::new_from_slices(&self.aes_key, &iv).ok()?;
        let len = decryptor.decrypt_padded::<NoPadding>(&mut buf).ok()?.len();
        buf.truncate(len);
        Some(buf)
    }

    /// `authenticate` with the shared secret's HMAC key.
    pub fn authenticate(&self, message: &[u8]) -> Vec<u8> {
        self.protocol.authenticate(&self.hmac_key, message)
    }
}

/// A random P-256 key (retrying the vanishingly rare invalid scalar).
pub fn random_secret(rng: &mut dyn Rng) -> Option<SecretKey> {
    for _ in 0..8 {
        let mut bytes = Zeroizing::new([0u8; 32]);
        rng.fill_bytes(bytes.as_mut());
        if let Ok(secret) = SecretKey::from_slice(bytes.as_ref()) {
            return Some(secret);
        }
    }
    None
}

/// A P-256 public key as a COSE_Key map: kty EC2, alg -25, crv P-256, x, y.
pub fn cose_key(public: &PublicKey) -> Value {
    let point = public.to_sec1_point(false);
    let raw = point.as_bytes();
    let x = raw.get(1..33).unwrap_or_default();
    let y = raw.get(33..65).unwrap_or_default();
    Value::Map(vec![
        int_entry(1, int(2)),
        int_entry(3, int(COSE_ECDH_ES_HKDF_256)),
        int_entry(-1, int(1)),
        int_entry(-2, bytes(x)),
        int_entry(-3, bytes(y)),
    ])
}

/// The P-256 public key in a COSE_Key map.
pub fn public_from_cose(value: &Value) -> Option<PublicKey> {
    let entries = map(value)?;
    let x = get_int_key(entries, -2).and_then(as_bytes)?;
    let y = get_int_key(entries, -3).and_then(as_bytes)?;
    if x.len() != 32 || y.len() != 32 {
        return None;
    }
    let mut sec1 = Vec::with_capacity(65);
    sec1.push(0x04);
    sec1.extend_from_slice(x);
    sec1.extend_from_slice(y);
    PublicKey::from_sec1_bytes(&sec1).ok()
}

/// ECDH between our secret and their public key: the shared secret.
pub fn agree(protocol: Protocol, secret: &SecretKey, peer: &PublicKey) -> SharedSecret {
    let shared = p256::ecdh::diffie_hellman(secret.to_nonzero_scalar(), peer.as_affine());
    SharedSecret::derive(protocol, shared.raw_secret_bytes())
}

/// A PIN/UV auth token and the protocol it belongs to.
pub struct PinToken {
    pub protocol: Protocol,
    pub token: Zeroizing<Vec<u8>>,
}

impl PinToken {
    /// `pinUvAuthParam` over `message` (the client data hash).
    pub fn param(&self, message: &[u8]) -> Vec<u8> {
        self.protocol.authenticate(&self.token, message)
    }
}

fn client_pin(
    key: &mut Authenticator,
    entries: Vec<(Value, Value)>,
    control: &mut Control<'_>,
) -> Result<Vec<(Value, Value)>, CtapError> {
    let answer = key.call(command::CLIENT_PIN, Some(Value::Map(entries)), control)?;
    match answer {
        Value::Map(entries) => Ok(entries),
        _ => Err(CtapError::Malformed("clientPIN")),
    }
}

/// Turn `pin` into a token: `getKeyAgreement`, then the token request with
/// `LEFT(SHA-256(pin), 16)` encrypted to the key. `permission` and `rp_id`
/// go along on keys that support permissions (CTAP 2.1); older keys get
/// the plain `getPinToken`.
pub fn pin_token(
    key: &mut Authenticator,
    pin: &str,
    permission: u8,
    rp_id: &str,
    rng: &mut dyn Rng,
    control: &mut Control<'_>,
) -> Result<PinToken, CtapError> {
    let protocol = Protocol::choose(&key.info().pin_protocols);
    let with_permissions = key.info().pin_uv_auth_token;
    let answer = client_pin(
        key,
        vec![
            int_entry(1, int(protocol.number())),
            int_entry(2, int(SUB_GET_KEY_AGREEMENT)),
        ],
        control,
    )?;
    let peer = get_int_key(&answer, 1)
        .and_then(public_from_cose)
        .ok_or(CtapError::Malformed("keyAgreement"))?;
    let secret = random_secret(rng).ok_or(CtapError::Malformed("random key"))?;
    let shared = agree(protocol, &secret, &peer);
    let pin_hash = sha256(pin.as_bytes());
    let pin_hash_enc = shared
        .encrypt(pin_hash.get(..16).unwrap_or_default(), rng)
        .ok_or(CtapError::Malformed("pinHashEnc"))?;
    let mut request = vec![
        int_entry(1, int(protocol.number())),
        int_entry(
            2,
            int(if with_permissions {
                SUB_GET_TOKEN_WITH_PERMISSIONS
            } else {
                SUB_GET_PIN_TOKEN
            }),
        ),
        int_entry(3, cose_key(&secret.public_key())),
        int_entry(6, bytes(&pin_hash_enc)),
    ];
    if with_permissions {
        request.push(int_entry(9, int(i64::from(permission))));
        request.push(int_entry(10, Value::Text(rp_id.to_string())));
    }
    let answer = client_pin(key, request, control)?;
    let encrypted = get_int_key(&answer, 2)
        .and_then(as_bytes)
        .ok_or(CtapError::Malformed("pinUvAuthToken"))?;
    let token = shared
        .decrypt(encrypted)
        .filter(|token| token.len() == 16 || token.len() == 32)
        .ok_or(CtapError::Malformed("pinUvAuthToken"))?;
    Ok(PinToken { protocol, token })
}
