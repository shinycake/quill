//! The primitives caBLE v2 is built from (tdesktop `cable_core.cpp`, there
//! with OpenSSL): SHA-256, HMAC-SHA-256, HKDF-SHA-256, AES-256-GCM, one
//! AES-256 block, and P-256 points and ECDH. All RustCrypto.
#![deny(clippy::indexing_slicing)]

use aes::Aes256;
use aes::cipher::{BlockCipherDecrypt, KeyInit as _};
use aes_gcm::Aes256Gcm;
use aes_gcm::aead::{Aead, Payload};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use p256::elliptic_curve::sec1::ToSec1Point;
use p256::{PublicKey, SecretKey};
use sha2_next::{Digest, Sha256};

/// An uncompressed P-256 point: 0x04, x, y.
pub const X962_LEN: usize = 65;
/// A compressed P-256 point: 0x02 or 0x03, x.
pub const COMPRESSED_LEN: usize = 33;
/// The AES-GCM tag.
pub const TAG_LEN: usize = 16;

pub fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let Ok(mut mac) = <Hmac<Sha256> as hmac::KeyInit>::new_from_slice(key) else {
        return [0; 32];
    };
    mac.update(data);
    mac.finalize().into_bytes().into()
}

/// HKDF-SHA-256 into `out`. An empty salt is the RFC 5869 default (HMAC
/// pads a short key with zeros either way). `false` only for an output
/// longer than HKDF allows, which no caller asks for.
pub fn hkdf_sha256(out: &mut [u8], ikm: &[u8], salt: &[u8], info: &[u8]) -> bool {
    Hkdf::<Sha256>::new(Some(salt), ikm)
        .expand(info, out)
        .is_ok()
}

/// AES-256-GCM with a 12-byte nonce: the ciphertext with its tag.
pub fn seal(key: &[u8; 32], nonce: &[u8; 12], plain: &[u8], aad: &[u8]) -> Option<Vec<u8>> {
    let cipher = <Aes256Gcm as aes_gcm::KeyInit>::new_from_slice(key).ok()?;
    cipher
        .encrypt(&(*nonce).into(), Payload { msg: plain, aad })
        .ok()
}

/// The other way; `None` when the tag does not match.
pub fn open(key: &[u8; 32], nonce: &[u8; 12], sealed: &[u8], aad: &[u8]) -> Option<Vec<u8>> {
    if sealed.len() < TAG_LEN {
        return None;
    }
    let cipher = <Aes256Gcm as aes_gcm::KeyInit>::new_from_slice(key).ok()?;
    cipher
        .decrypt(&(*nonce).into(), Payload { msg: sealed, aad })
        .ok()
}

/// One AES-256 block, decrypted (the advert's EID).
pub fn aes256_decrypt_block(key: &[u8], block: &[u8; 16]) -> Option<[u8; 16]> {
    let cipher = Aes256::new_from_slice(key).ok()?;
    let mut block = aes::Block::from(*block);
    cipher.decrypt_block(&mut block);
    Some(block.into())
}

/// One AES-256 block, encrypted (what a phone does to its EID; tests).
#[cfg(test)]
pub fn aes256_encrypt_block(key: &[u8], block: &[u8; 16]) -> Option<[u8; 16]> {
    use aes::cipher::BlockCipherEncrypt;
    let cipher = Aes256::new_from_slice(key).ok()?;
    let mut block = aes::Block::from(*block);
    cipher.encrypt_block(&mut block);
    Some(block.into())
}

/// The X9.62 encoding of a public key.
pub fn x962(public: &PublicKey, compressed: bool) -> Vec<u8> {
    public.to_sec1_point(compressed).as_bytes().to_vec()
}

/// The ECDH shared x coordinate with a peer's X9.62 point; `None` for a
/// point that is not on the curve.
pub fn ecdh_x(secret: &SecretKey, peer_x962: &[u8]) -> Option<[u8; 32]> {
    let peer = PublicKey::from_sec1_bytes(peer_x962).ok()?;
    let shared = p256::ecdh::diffie_hellman(secret.to_nonzero_scalar(), peer.as_affine());
    shared.raw_secret_bytes().as_slice().try_into().ok()
}
