//! One phone ceremony on the worker thread (tdesktop `cable_ceremony.cpp`
//! `Start`, `ShowBox`, `OnAdvert`, `HandleTunnelData`, `SendCtapRequest`,
//! `HandleReply`, `Finish`): scan, show the QR sheet, wait for the
//! phone's advertisement or "Use a security key", open the tunnel, run
//! the handshake and one CTAP2 request.
#![deny(clippy::indexing_slicing)]

use super::ble::{ScanEvent, Scanner};
use super::eid::{self, EID_KEY_LEN};
use super::messages::{self, Reply};
use super::noise::{Crypter, Initiator};
use super::qr::{self, Derived, QrKey};
use super::tunnel::{Stop, Tunnel, TunnelError};
use super::{DEFAULT_TIMEOUT, Environment};
use crate::webauthn::ceremony::{CeremonyCommand, CeremonyEvent, HybridQr, Link};
use crate::webauthn::ctap2::commands::{
    none_attestation_object, parse_got_assertion, parse_made_credential,
};
use crate::webauthn::ctap2::pin::random_secret;
use crate::webauthn::{
    Assertion, CeremonyError, CreationOptions, Registration, RequestOptions, log,
};
use ciborium::Value;
use std::collections::HashSet;
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

/// How long the worker waits for a scan event before it looks at Cancel,
/// the deadline and the dialog's commands again.
const POLL: Duration = Duration::from_millis(100);

/// What the phone is asked for.
#[derive(Clone, Copy)]
pub(super) enum Kind<'a> {
    Register(&'a CreationOptions),
    Login(&'a RequestOptions),
}

impl Kind<'_> {
    fn timeout(self) -> Duration {
        let ms = match self {
            Self::Register(options) => options.timeout_ms,
            Self::Login(options) => options.timeout_ms,
        };
        if ms == 0 {
            DEFAULT_TIMEOUT
        } else {
            Duration::from_millis(u64::from(ms))
        }
    }

    fn client_data_json(self) -> String {
        match self {
            Self::Register(options) => options.client_data_json(),
            Self::Login(options) => options.client_data_json(),
        }
    }

    fn request(self, client_data_hash: &[u8]) -> Option<Vec<u8>> {
        match self {
            Self::Register(options) => messages::make_credential_request(client_data_hash, options),
            Self::Login(options) => messages::get_assertion_request(client_data_hash, options),
        }
    }

    /// The phone's answer as the ceremony's last event.
    fn finish(
        self,
        answer: &Value,
        client_data_json: String,
    ) -> Result<CeremonyEvent, CeremonyError> {
        let malformed = |error| {
            log(&format!("phone answer unreadable: {error}"));
            CeremonyError::Phone
        };
        match self {
            Self::Register(_) => {
                let made = parse_made_credential(answer).map_err(malformed)?;
                Ok(CeremonyEvent::Registered(Registration {
                    credential_id: made.credential_id,
                    attestation_object: none_attestation_object(&made.auth_data),
                    client_data_json,
                }))
            }
            Self::Login(options) => {
                let got =
                    parse_got_assertion(answer, &options.allow_credentials).map_err(malformed)?;
                Ok(CeremonyEvent::SignedIn(Assertion {
                    credential_id: got.credential_id,
                    authenticator_data: got.auth_data,
                    signature: got.signature,
                    user_handle: got.user_handle,
                    client_data_json,
                }))
            }
        }
    }
}

/// How the phone ceremony ended.
pub(super) enum End {
    /// The ceremony's last event.
    Event(CeremonyEvent),
    /// "Use a security key" with a key present: the USB path takes over.
    SecurityKey,
}

/// Stops the scanner when the scan phase ends, whichever way.
struct ScanGuard(Option<Box<dyn Scanner>>);

impl ScanGuard {
    fn stop(&mut self) {
        if let Some(mut scanner) = self.0.take() {
            scanner.stop();
        }
    }
}

impl Drop for ScanGuard {
    fn drop(&mut self) {
        self.stop();
    }
}

pub(super) fn run(kind: Kind<'_>, env: &mut Environment, link: &Link) -> End {
    match run_phone(kind, env, link) {
        Ok(end) => end,
        Err(error) => End::Event(CeremonyEvent::Failed(error)),
    }
}

fn run_phone(kind: Kind<'_>, env: &mut Environment, link: &Link) -> Result<End, CeremonyError> {
    let deadline = Instant::now() + kind.timeout();
    let qr = QrKey::generate(env.rng.as_mut()).ok_or(CeremonyError::Phone)?;
    let eid_key = qr::derive::<EID_KEY_LEN>(qr.secret(), &[], Derived::EidKey);

    let (scan_tx, scan_rx) = channel();
    let mut scanner = env.scanner.take().and_then(|make| make());
    let started = scanner
        .as_mut()
        .is_some_and(|scanner| scanner.start(scan_tx));
    let mut guard = ScanGuard(scanner);
    let mut early = Vec::new();
    let available = started && probe(&scan_rx, env.bluetooth_probe, link, &mut early)?;
    if !available {
        log("bluetooth unavailable");
    }
    let code = if available {
        let make_credential = matches!(kind, Kind::Register(_));
        let text =
            qr::encode_contents(&qr, make_credential, env.now_unix).ok_or(CeremonyError::Phone)?;
        Some(HybridQr(text))
    } else {
        None
    };
    let _ = link.events.send(CeremonyEvent::Hybrid { qr: code });

    let mut seen = HashSet::new();
    let eid = loop {
        if link.cancelled() {
            return Err(CeremonyError::Cancelled);
        }
        if Instant::now() >= deadline {
            log("phone ceremony timed out");
            return Err(CeremonyError::Phone);
        }
        while let Ok(command) = link.commands.try_recv() {
            match command {
                CeremonyCommand::UseSecurityKey => {
                    // tdesktop `securityKeyChosen`: a toast when no key is
                    // there, else the key path.
                    if env.keys.open_all().is_empty() {
                        let _ = link.events.send(CeremonyEvent::NoKey);
                    } else {
                        guard.stop();
                        return Ok(End::SecurityKey);
                    }
                }
            }
        }
        let advert = match early.pop() {
            Some(advert) => Some(advert),
            None => match scan_rx.recv_timeout(POLL) {
                Ok(ScanEvent::Advert(advert)) => Some(advert),
                Ok(ScanEvent::Available(_)) => None,
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => {
                    // No scanner (or it stopped): only Cancel, a key or
                    // the deadline end this now.
                    std::thread::sleep(POLL);
                    None
                }
            },
        };
        let Some(advert) = advert else {
            continue;
        };
        if !seen.insert(advert.clone()) {
            continue;
        }
        if let Some(eid) = eid::decrypt_advert(&advert, &eid_key) {
            break eid;
        }
    };
    guard.stop();
    let _ = link.events.send(CeremonyEvent::PhoneConnecting);

    let parts = eid::parts(&eid);
    let domain = eid::tunnel_server_domain(parts.tunnel_server_domain).ok_or_else(|| {
        log("unknown tunnel server");
        CeremonyError::Phone
    })?;
    let psk = qr::derive::<32>(qr.secret(), &eid, Derived::Psk);
    let tunnel_id = qr::derive::<16>(qr.secret(), &[], Derived::TunnelId);
    let path = format!(
        "/cable/connect/{}/{}",
        eid::hex_lower(&parts.routing_id),
        eid::hex_lower(tunnel_id.as_ref())
    );
    let cancelled = || link.cancelled();
    let stop = Stop {
        deadline,
        cancelled: &cancelled,
    };
    let mut tunnel = env
        .connector
        .connect(&domain, &path, &stop)
        .map_err(tunnel_error)?;
    let ephemeral = random_secret(env.rng.as_mut()).ok_or(CeremonyError::Phone)?;
    let mut initiator = Initiator::new(&psk, &qr.identity);
    let result = initiator
        .initial_message(ephemeral)
        .ok_or(CeremonyError::Phone)
        .and_then(|first| converse(kind, initiator, &first, tunnel.as_mut(), &stop, link));
    tunnel.close();
    result.map(End::Event)
}

/// Wait for the scanner to say whether Bluetooth works, up to `limit`;
/// advertisements that come first are kept.
fn probe(
    events: &Receiver<ScanEvent>,
    limit: Duration,
    link: &Link,
    early: &mut Vec<Vec<u8>>,
) -> Result<bool, CeremonyError> {
    let until = Instant::now() + limit;
    loop {
        if link.cancelled() {
            return Err(CeremonyError::Cancelled);
        }
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Ok(false);
        }
        match events.recv_timeout(left.min(POLL)) {
            Ok(ScanEvent::Available(available)) => return Ok(available),
            Ok(ScanEvent::Advert(advert)) => early.push(advert),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Ok(false),
        }
    }
}

fn tunnel_error(error: TunnelError) -> CeremonyError {
    match error {
        TunnelError::Cancelled => CeremonyError::Cancelled,
        other => {
            log(&format!("tunnel: {other:?}"));
            CeremonyError::Phone
        }
    }
}

/// The handshake, the phone's first message, the request and the answer.
fn converse(
    kind: Kind<'_>,
    initiator: Initiator<'_>,
    first: &[u8],
    tunnel: &mut dyn Tunnel,
    stop: &Stop<'_>,
    link: &Link,
) -> Result<CeremonyEvent, CeremonyError> {
    tunnel.send(first, stop).map_err(tunnel_error)?;
    let response = tunnel.recv(stop).map_err(tunnel_error)?;
    let handshake = initiator.process_response(&response).ok_or_else(|| {
        log("handshake failed");
        CeremonyError::Phone
    })?;
    let mut crypter: Crypter = handshake.crypter;
    let sealed = tunnel.recv(stop).map_err(tunnel_error)?;
    let post = crypter
        .decrypt(&sealed)
        .and_then(|plain| messages::parse_post_handshake(&plain))
        .filter(|post| post.supports_ctap)
        .ok_or_else(|| {
            log("phone's first message unreadable or without ctap");
            CeremonyError::Phone
        })?;
    let client_data_json = kind.client_data_json();
    let hash = crate::webauthn::hybrid::crypto::sha256(client_data_json.as_bytes());
    let request = kind.request(&hash).ok_or(CeremonyError::Phone)?;
    let sealed = crypter
        .encrypt(&messages::frame_request(&request, post.protocol_revision))
        .ok_or(CeremonyError::Phone)?;
    let _ = link.events.send(CeremonyEvent::PhoneContinue);
    tunnel.send(&sealed, stop).map_err(tunnel_error)?;
    loop {
        let sealed = tunnel.recv(stop).map_err(tunnel_error)?;
        let plain = crypter.decrypt(&sealed).ok_or_else(|| {
            log("phone message did not decrypt");
            CeremonyError::Phone
        })?;
        match messages::classify_reply(plain, post.protocol_revision) {
            Reply::Update => {}
            Reply::Unexpected => {
                log("unexpected phone message");
                return Err(CeremonyError::Phone);
            }
            Reply::Ctap(payload) => {
                let answer = messages::ctap_answer(&payload).map_err(|status| {
                    log(&format!("phone answered with status {status:?}"));
                    CeremonyError::Phone
                })?;
                return kind.finish(&answer, client_data_json);
            }
        }
    }
}
