//! Listening for the phone's Bluetooth advertisement (tdesktop
//! `cable_scanner.h` and its macOS / Linux / Windows backends). A
//! scanner reports once whether Bluetooth is usable, then the service
//! data of every caBLE advertisement it sees (service UUID FDE2, Google's,
//! or FFF9, FIDO's).
#![deny(clippy::indexing_slicing)]

use std::sync::mpsc::Sender;

/// The 16-bit service UUIDs a caBLE advertisement uses.
pub const GOOGLE_CABLE_UUID: u16 = 0xFDE2;
pub const FIDO_CABLE_UUID: u16 = 0xFFF9;

/// What a scanner tells the ceremony.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanEvent {
    /// Whether Bluetooth can scan (sent once).
    Available(bool),
    /// The service data of one caBLE advertisement.
    Advert(Vec<u8>),
}

/// One scan, run on the ceremony's worker thread: made there, started,
/// stopped and dropped there.
pub trait Scanner {
    /// Start scanning; `false` when it cannot start at all.
    fn start(&mut self, events: Sender<ScanEvent>) -> bool;
    fn stop(&mut self);
}

/// Makes the platform's scanner on the worker thread.
pub type ScannerFactory = Box<dyn FnOnce() -> Option<Box<dyn Scanner>> + Send>;

/// Whether this build can look for a phone: CoreBluetooth on macOS,
/// BlueZ over D-Bus on Linux (both with the app's UI). Windows shows
/// phones in its own WebAuthn dialog.
pub fn supported() -> bool {
    cfg!(all(
        any(target_os = "macos", target_os = "linux"),
        feature = "ui"
    ))
}

/// The platform's scanner, made where it is called.
pub fn platform_scanner() -> Option<Box<dyn Scanner>> {
    #[cfg(all(target_os = "macos", feature = "ui"))]
    {
        Some(Box::new(super::ble_macos::MacScanner::default()))
    }
    #[cfg(all(target_os = "linux", feature = "ui"))]
    {
        Some(Box::new(super::ble_linux::LinuxScanner::default()))
    }
    #[cfg(not(all(any(target_os = "macos", target_os = "linux"), feature = "ui")))]
    {
        None
    }
}

/// Whether a 128-bit UUID string names a caBLE service
/// (`0000fde2-0000-1000-8000-00805f9b34fb` or the FFF9 one, any case).
pub fn is_cable_uuid(uuid: &str) -> bool {
    [GOOGLE_CABLE_UUID, FIDO_CABLE_UUID].iter().any(|short| {
        uuid.eq_ignore_ascii_case(&format!("0000{short:04x}-0000-1000-8000-00805f9b34fb"))
    })
}
