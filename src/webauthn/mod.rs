//! Quill as a WebAuthn client for Telegram passkeys
//! (docs/decisions/passkeys-ceremony.md).
//!
//! TDLib hands over the WebAuthn options as JSON (`getPasskeyParameters`,
//! `getAuthenticationPasskeyParameters`); Quill parses them the way Telegram
//! Desktop does (`options`), builds `clientDataJSON` itself, runs the
//! ceremony with a USB security key over CTAP2 (`ctap2`, `ceremony`) and
//! returns the result to TDLib (`addLoginPasskey`,
//! `checkAuthenticationPasskey`). UI-independent: the dialog is
//! `ui::security::passkey_ceremony`.
#![deny(clippy::indexing_slicing)]

pub mod ceremony;
pub mod ctap2;
pub mod options;

pub use ceremony::{Ceremony, CeremonyEvent, DeviceSource};
pub use options::{CreationOptions, RequestOptions, UserVerification};

/// What a finished registration hands to `addLoginPasskey`.
#[derive(Clone, PartialEq, Eq)]
pub struct Registration {
    pub credential_id: Vec<u8>,
    /// `{"fmt": "none", "attStmt": {}, "authData": ..}`, as tdesktop sends.
    pub attestation_object: Vec<u8>,
    pub client_data_json: String,
}

impl std::fmt::Debug for Registration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registration")
            .field("credential_id_len", &self.credential_id.len())
            .field("attestation_object_len", &self.attestation_object.len())
            .finish_non_exhaustive()
    }
}

/// What a finished sign-in hands to `checkAuthenticationPasskey`.
#[derive(Clone, PartialEq, Eq)]
pub struct Assertion {
    pub credential_id: Vec<u8>,
    pub authenticator_data: Vec<u8>,
    pub signature: Vec<u8>,
    pub user_handle: Vec<u8>,
    pub client_data_json: String,
}

impl std::fmt::Debug for Assertion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Assertion")
            .field("credential_id_len", &self.credential_id.len())
            .finish_non_exhaustive()
    }
}

/// Why a ceremony ended without a result. The PIN cases never end a
/// ceremony: they ask for a PIN through [`CeremonyEvent::NeedPin`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CeremonyError {
    /// The user cancelled.
    Cancelled,
    /// No security key was found before the request timed out.
    NoDevice,
    /// The key has no passkey for this account (sign-in).
    NoCredentials,
    /// The key's PIN is blocked.
    PinBlocked,
    /// Anything else; the text is for the log only.
    Other(String),
}

/// Whether this build can run a ceremony: a USB security key over CTAP2
/// on macOS and Linux. Windows goes through `webauthn.dll` (later work).
pub const fn supported() -> bool {
    cfg!(any(target_os = "macos", target_os = "linux"))
}

/// One line in the log for a failed step (tdesktop `LOG(("Passkey Error:
/// ..."))`). Never carries a PIN, a challenge or key material.
pub fn log(line: &str) {
    eprintln!("quill: passkey: {line}");
}
