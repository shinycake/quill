//! The tunnel's Noise handshake and the encryption after it (tdesktop
//! `cable_core.cpp` `Noise`, `Crypter`, `HandshakeInitiator`,
//! `RespondToHandshake`; Chromium's caBLE v2 does the same).
//!
//! QR-initiated hybrid uses `Noise_KNpsk0_P256_AESGCM_SHA256`: Quill's QR
//! identity is known to the phone, the PSK comes from the QR secret and
//! the phone's advertisement. After the handshake each side has a read
//! and a write key; messages are padded to 32 bytes and sealed with
//! AES-256-GCM under a counter nonce.
#![deny(clippy::indexing_slicing)]

use super::crypto::{self, TAG_LEN, X962_LEN, ecdh_x, x962};
use p256::SecretKey;
use zeroize::Zeroizing;

/// The handshake answer: the phone's ephemeral point and an empty
/// message's tag.
pub const RESPONSE_LEN: usize = X962_LEN + TAG_LEN;
/// The highest message number either way (tdesktop `kMaxSequence`).
const MAX_SEQUENCE: u32 = (1 << 24) - 1;
/// Messages are padded to a multiple of this.
const PADDING_GRANULARITY: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandshakeType {
    /// QR-initiated: the phone knows Quill's identity key.
    KNpsk0,
    /// Linked phones (not used by Telegram Desktop).
    NKpsk0,
}

/// The Noise symmetric state.
pub struct Noise {
    chaining_key: Zeroizing<[u8; 32]>,
    h: [u8; 32],
    symmetric_key: Zeroizing<[u8; 32]>,
    symmetric_nonce: u32,
}

impl Noise {
    pub fn new(kind: HandshakeType) -> Self {
        let name: &[u8] = match kind {
            HandshakeType::KNpsk0 => b"Noise_KNpsk0_P256_AESGCM_SHA256",
            HandshakeType::NKpsk0 => b"Noise_NKpsk0_P256_AESGCM_SHA256",
        };
        let mut chaining_key = [0u8; 32];
        for (slot, byte) in chaining_key.iter_mut().zip(name) {
            *slot = *byte;
        }
        Self {
            chaining_key: Zeroizing::new(chaining_key),
            h: chaining_key,
            symmetric_key: Zeroizing::new([0; 32]),
            symmetric_nonce: 0,
        }
    }

    pub fn mix_hash(&mut self, data: &[u8]) {
        let mut input = Vec::with_capacity(32 + data.len());
        input.extend_from_slice(&self.h);
        input.extend_from_slice(data);
        self.h = crypto::sha256(&input);
    }

    pub fn mix_key(&mut self, ikm: &[u8]) {
        let mut out = Zeroizing::new([0u8; 64]);
        crypto::hkdf_sha256(out.as_mut(), ikm, self.chaining_key.as_ref(), &[]);
        let (chaining, key) = out.split_at(32);
        self.chaining_key.copy_from_slice(chaining);
        self.initialize_key(key);
    }

    pub fn mix_key_and_hash(&mut self, ikm: &[u8]) {
        let mut out = Zeroizing::new([0u8; 96]);
        crypto::hkdf_sha256(out.as_mut(), ikm, self.chaining_key.as_ref(), &[]);
        let (chaining, rest) = out.split_at(32);
        let (hash, key) = rest.split_at(32);
        self.chaining_key.copy_from_slice(chaining);
        self.mix_hash(hash);
        self.initialize_key(key);
    }

    fn initialize_key(&mut self, key: &[u8]) {
        self.symmetric_key.copy_from_slice(key);
        self.symmetric_nonce = 0;
    }

    /// The handshake nonce: the counter big-endian in the first four
    /// bytes (as Chromium).
    fn next_nonce(&mut self) -> [u8; 12] {
        let [a, b, c, d] = self.symmetric_nonce.to_be_bytes();
        self.symmetric_nonce = self.symmetric_nonce.wrapping_add(1);
        [a, b, c, d, 0, 0, 0, 0, 0, 0, 0, 0]
    }

    pub fn encrypt_and_hash(&mut self, plain: &[u8]) -> Option<Vec<u8>> {
        let nonce = self.next_nonce();
        let sealed = crypto::seal(&self.symmetric_key, &nonce, plain, &self.h)?;
        self.mix_hash(&sealed);
        Some(sealed)
    }

    pub fn decrypt_and_hash(&mut self, sealed: &[u8]) -> Option<Vec<u8>> {
        let nonce = self.next_nonce();
        let plain = crypto::open(&self.symmetric_key, &nonce, sealed, &self.h)?;
        self.mix_hash(sealed);
        Some(plain)
    }

    /// The two traffic keys: the initiator writes with the first.
    pub fn traffic_keys(&self) -> (Zeroizing<[u8; 32]>, Zeroizing<[u8; 32]>) {
        let mut out = Zeroizing::new([0u8; 64]);
        crypto::hkdf_sha256(out.as_mut(), &[], self.chaining_key.as_ref(), &[]);
        let mut first = Zeroizing::new([0u8; 32]);
        let mut second = Zeroizing::new([0u8; 32]);
        let (a, b) = out.split_at(32);
        first.copy_from_slice(a);
        second.copy_from_slice(b);
        (first, second)
    }

    pub fn handshake_hash(&self) -> [u8; 32] {
        self.h
    }
}

/// Seals and opens the messages after the handshake.
pub struct Crypter {
    read_key: Zeroizing<[u8; 32]>,
    write_key: Zeroizing<[u8; 32]>,
    read_sequence: u32,
    write_sequence: u32,
}

/// The message nonce: the counter big-endian in the last four bytes.
fn sequence_nonce(sequence: u32) -> [u8; 12] {
    let [a, b, c, d] = sequence.to_be_bytes();
    [0, 0, 0, 0, 0, 0, 0, 0, a, b, c, d]
}

impl Crypter {
    pub fn new(read_key: Zeroizing<[u8; 32]>, write_key: Zeroizing<[u8; 32]>) -> Self {
        Self {
            read_key,
            write_key,
            read_sequence: 0,
            write_sequence: 0,
        }
    }

    /// Pad to a multiple of 32 (the last byte says how many zero bytes
    /// came before it) and seal.
    pub fn encrypt(&mut self, plain: &[u8]) -> Option<Vec<u8>> {
        if self.write_sequence > MAX_SEQUENCE {
            return None;
        }
        let padded_len = plain
            .len()
            .checked_add(1)?
            .checked_next_multiple_of(PADDING_GRANULARITY)?;
        let padding = padded_len.checked_sub(plain.len())?.checked_sub(1)?;
        let mut padded = Vec::with_capacity(padded_len);
        padded.extend_from_slice(plain);
        padded.resize(padded_len.checked_sub(1)?, 0);
        padded.push(u8::try_from(padding).ok()?);
        let nonce = sequence_nonce(self.write_sequence);
        self.write_sequence = self.write_sequence.saturating_add(1);
        crypto::seal(&self.write_key, &nonce, &padded, &[])
    }

    /// Open and strip the padding.
    pub fn decrypt(&mut self, sealed: &[u8]) -> Option<Vec<u8>> {
        if self.read_sequence > MAX_SEQUENCE {
            return None;
        }
        let nonce = sequence_nonce(self.read_sequence);
        let mut plain = crypto::open(&self.read_key, &nonce, sealed, &[])?;
        self.read_sequence = self.read_sequence.saturating_add(1);
        let padding = usize::from(*plain.last()?);
        let len = plain.len().checked_sub(padding)?.checked_sub(1)?;
        plain.truncate(len);
        Some(plain)
    }
}

/// What a finished handshake gives.
pub struct Handshake {
    pub crypter: Crypter,
    pub hash: [u8; 32],
}

/// Quill's side of the handshake (tdesktop `HandshakeInitiator`).
pub struct Initiator<'a> {
    psk: Zeroizing<[u8; 32]>,
    identity: &'a SecretKey,
    ephemeral: Option<SecretKey>,
    noise: Noise,
}

/// The prologue Telegram Desktop mixes in first.
const PROLOGUE: [u8; 1] = [1];

impl<'a> Initiator<'a> {
    pub fn new(psk: &[u8; 32], identity: &'a SecretKey) -> Self {
        Self {
            psk: Zeroizing::new(*psk),
            identity,
            ephemeral: None,
            noise: Noise::new(HandshakeType::KNpsk0),
        }
    }

    /// The first message: the ephemeral point and an empty message's tag.
    /// `ephemeral` is random in a ceremony, fixed in tests.
    pub fn initial_message(&mut self, ephemeral: SecretKey) -> Option<Vec<u8>> {
        self.noise = Noise::new(HandshakeType::KNpsk0);
        self.noise.mix_hash(&PROLOGUE);
        self.noise
            .mix_hash(&x962(&self.identity.public_key(), false));
        self.noise.mix_key_and_hash(self.psk.as_ref());
        let public = x962(&ephemeral.public_key(), false);
        self.noise.mix_hash(&public);
        self.noise.mix_key(&public);
        let tag = self.noise.encrypt_and_hash(&[])?;
        self.ephemeral = Some(ephemeral);
        let mut message = public;
        message.extend_from_slice(&tag);
        Some(message)
    }

    /// The phone's answer: its ephemeral point (ee and se) and an empty
    /// message.
    pub fn process_response(mut self, response: &[u8]) -> Option<Handshake> {
        if response.len() != RESPONSE_LEN {
            return None;
        }
        let ephemeral = self.ephemeral.take()?;
        let (peer, sealed) = response.split_at(X962_LEN);
        let ee = Zeroizing::new(ecdh_x(&ephemeral, peer)?);
        self.noise.mix_hash(peer);
        self.noise.mix_key(peer);
        self.noise.mix_key(ee.as_ref());
        let se = Zeroizing::new(ecdh_x(self.identity, peer)?);
        self.noise.mix_key(se.as_ref());
        let plain = self.noise.decrypt_and_hash(sealed)?;
        if !plain.is_empty() {
            return None;
        }
        let (write, read) = self.noise.traffic_keys();
        Some(Handshake {
            crypter: Crypter::new(read, write),
            hash: self.noise.handshake_hash(),
        })
    }
}

/// The phone's side (tdesktop `RespondToHandshake`): tests and the fake
/// phone.
#[cfg(test)]
pub fn respond(
    psk: &[u8; 32],
    peer_identity_x962: &[u8],
    message: &[u8],
    ephemeral: &SecretKey,
) -> Option<(Vec<u8>, Handshake)> {
    let peer = message.get(..X962_LEN)?;
    let sealed = message.get(X962_LEN..)?;
    let mut noise = Noise::new(HandshakeType::KNpsk0);
    noise.mix_hash(&PROLOGUE);
    noise.mix_hash(peer_identity_x962);
    noise.mix_key_and_hash(psk);
    noise.mix_hash(peer);
    noise.mix_key(peer);
    if !noise.decrypt_and_hash(sealed)?.is_empty() {
        return None;
    }
    let public = x962(&ephemeral.public_key(), false);
    noise.mix_hash(&public);
    noise.mix_key(&public);
    noise.mix_key(&ecdh_x(ephemeral, peer)?);
    noise.mix_key(&ecdh_x(ephemeral, peer_identity_x962)?);
    let tag = noise.encrypt_and_hash(&[])?;
    let mut response = public;
    response.extend_from_slice(&tag);
    let (read, write) = noise.traffic_keys();
    Some((
        response,
        Handshake {
            crypter: Crypter::new(read, write),
            hash: noise.handshake_hash(),
        },
    ))
}
