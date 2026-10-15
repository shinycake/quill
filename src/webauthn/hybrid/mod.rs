//! Passkeys from a phone: hybrid transport, also called caBLE v2
//! (docs/decisions/passkeys-hybrid.md). The flow is Telegram Desktop's
//! `webauthn/cable_*.cpp`, which follows Chromium:
//!
//! 1. Quill makes a P-256 identity and a secret and shows them as a
//!    `FIDO:/` QR code (`qr`).
//! 2. The phone scans it, opens a tunnel on a tunnel server, and
//!    advertises over Bluetooth an EID encrypted with a key derived from
//!    the secret; Quill scans for it (`ble`) and decrypts it (`eid`) to
//!    learn the server and the tunnel's routing id.
//! 3. Quill opens the same tunnel (`tunnel`, a websocket over TLS), runs a
//!    Noise KNpsk0 handshake with the phone (`noise`), reads the phone's
//!    `getInfo`, sends one CTAP2 request and reads the answer
//!    (`messages`).
//!
//! `flow` runs that on the ceremony's worker thread and reports through
//! the same `CeremonyEvent`s as a security key; "Use a security key"
//! hands the same ceremony over to the USB path.
#![deny(clippy::indexing_slicing)]

pub mod ble;
#[cfg(all(target_os = "linux", feature = "ui"))]
mod ble_linux;
#[cfg(all(target_os = "macos", feature = "ui"))]
mod ble_macos;
pub mod crypto;
pub mod eid;
mod flow;
pub mod messages;
pub mod noise;
pub mod qr;
pub mod tunnel;

#[cfg(test)]
mod fake_phone;
#[cfg(test)]
mod tests;

use super::ceremony::{self, Ceremony, CeremonyEvent, DeviceSource, UsbKeys};
use super::{CreationOptions, RequestOptions};
use ble::ScannerFactory;
use rand::RngCore;
use std::time::Duration;
use tunnel::Connector;

/// How long Quill waits to learn whether Bluetooth works before it shows
/// the sheet without the QR code (tdesktop `kBluetoothProbeMs`).
pub const BLUETOOTH_PROBE: Duration = Duration::from_millis(1500);
/// The phone ceremony's limit when the options have none (tdesktop
/// `kCeremonyTimeoutMs`).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(180);

/// Everything the phone ceremony reaches outside itself, so tests can
/// replace each piece.
pub struct Environment {
    /// Makes the Bluetooth scanner on the worker thread.
    pub scanner: Option<ScannerFactory>,
    /// Opens the tunnel.
    pub connector: Box<dyn Connector>,
    /// The USB keys, for "Use a security key".
    pub keys: Box<dyn DeviceSource>,
    pub rng: Box<dyn RngCore + Send>,
    /// The QR code's timestamp.
    pub now_unix: i64,
    pub bluetooth_probe: Duration,
}

impl Environment {
    /// This computer: its Bluetooth, the real tunnel servers, its USB
    /// keys.
    pub fn platform() -> Self {
        let now_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX))
            .unwrap_or(0);
        Self {
            scanner: Some(Box::new(ble::platform_scanner)),
            connector: Box::new(tunnel::TlsConnector),
            keys: Box::new(UsbKeys),
            rng: Box::new(rand::rngs::OsRng),
            now_unix,
            bluetooth_probe: BLUETOOTH_PROBE,
        }
    }
}

/// Create a passkey on a phone, or on a security key chosen from the QR
/// sheet (tdesktop `RegisterViaCable`).
pub fn register(options: CreationOptions, mut env: Environment) -> Ceremony {
    Ceremony::spawn(
        move |link| match flow::run(flow::Kind::Register(&options), &mut env, link) {
            flow::End::Event(event) => event,
            flow::End::SecurityKey => {
                let _ = link.events.send(CeremonyEvent::UseKey);
                match ceremony::run_register(&options, env.keys.as_mut(), link) {
                    Ok(registration) => CeremonyEvent::Registered(registration),
                    Err(error) => CeremonyEvent::Failed(error),
                }
            }
        },
    )
}

/// Sign in with a passkey on a phone, or on a security key chosen from
/// the QR sheet (tdesktop `LoginViaCable`).
pub fn login(options: RequestOptions, mut env: Environment) -> Ceremony {
    Ceremony::spawn(
        move |link| match flow::run(flow::Kind::Login(&options), &mut env, link) {
            flow::End::Event(event) => event,
            flow::End::SecurityKey => {
                let _ = link.events.send(CeremonyEvent::UseKey);
                match ceremony::run_login(&options, env.keys.as_mut(), link) {
                    Ok(assertion) => CeremonyEvent::SignedIn(assertion),
                    Err(error) => CeremonyEvent::Failed(error),
                }
            }
        },
    )
}
