//! CTAP2 over USB HID (FIDO CTAP 2.1, sections 6 and 11.2): the CTAPHID
//! framing (`ctaphid`), `authenticatorGetInfo` (`info`), the PIN/UV auth
//! protocols one and two (`pin`), `authenticatorMakeCredential` and
//! `authenticatorGetAssertion` (`commands`), and the HID devices of the
//! platform (`hid`). Only CTAP2: Telegram's passkeys are discoverable
//! credentials, which CTAP1/U2F cannot make.
#![deny(clippy::indexing_slicing)]

pub mod cbor;
pub mod commands;
pub mod ctaphid;
pub mod hid;
pub mod info;
pub mod pin;

#[cfg(test)]
pub(crate) mod fake;
#[cfg(test)]
mod tests;

pub use ctaphid::{Channel, Control, HidDevice, HidError};
pub use info::Info;

/// CTAP2 status codes Quill tells apart (CTAP 2.1 section 8.2).
pub mod status {
    pub const OK: u8 = 0x00;
    pub const INVALID_COMMAND: u8 = 0x01;
    pub const CREDENTIAL_EXCLUDED: u8 = 0x19;
    pub const UNSUPPORTED_ALGORITHM: u8 = 0x26;
    pub const OPERATION_DENIED: u8 = 0x27;
    pub const UNSUPPORTED_OPTION: u8 = 0x2B;
    pub const INVALID_OPTION: u8 = 0x2C;
    pub const KEEPALIVE_CANCEL: u8 = 0x2D;
    pub const NO_CREDENTIALS: u8 = 0x2E;
    pub const USER_ACTION_TIMEOUT: u8 = 0x2F;
    pub const PIN_INVALID: u8 = 0x31;
    pub const PIN_BLOCKED: u8 = 0x32;
    pub const PIN_AUTH_INVALID: u8 = 0x33;
    pub const PIN_AUTH_BLOCKED: u8 = 0x34;
    pub const PIN_NOT_SET: u8 = 0x35;
    /// `CTAP2_ERR_PUAT_REQUIRED`, called `PIN_REQUIRED` in CTAP 2.0.
    pub const PIN_REQUIRED: u8 = 0x36;
}

/// Command bytes of the CTAP2 CBOR messages.
pub mod command {
    pub const MAKE_CREDENTIAL: u8 = 0x01;
    pub const GET_ASSERTION: u8 = 0x02;
    pub const GET_INFO: u8 = 0x04;
    pub const CLIENT_PIN: u8 = 0x06;
}

/// A CTAP2 exchange that did not produce an answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CtapError {
    /// The HID transport failed, timed out or was cancelled.
    Hid(HidError),
    /// The authenticator answered with this status code.
    Status(u8),
    /// The answer could not be read.
    Malformed(&'static str),
}

impl From<HidError> for CtapError {
    fn from(error: HidError) -> Self {
        Self::Hid(error)
    }
}

impl std::fmt::Display for CtapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hid(error) => write!(f, "hid: {error:?}"),
            Self::Status(code) => write!(f, "ctap status 0x{code:02x}"),
            Self::Malformed(what) => write!(f, "malformed answer: {what}"),
        }
    }
}

/// One authenticator: a CTAPHID channel and its `getInfo`.
pub struct Authenticator {
    channel: Channel,
    info: Info,
}

impl Authenticator {
    /// Allocate a channel and read `getInfo`.
    pub fn open(device: Box<dyn HidDevice>, control: &mut Control<'_>) -> Result<Self, CtapError> {
        let mut channel = Channel::open(device, control)?;
        let answer = channel.cbor(command::GET_INFO, None, control)?;
        let info = Info::parse(&answer).ok_or(CtapError::Malformed("getInfo"))?;
        Ok(Self { channel, info })
    }

    pub fn info(&self) -> &Info {
        &self.info
    }

    /// Send one CBOR command and decode the answer's map (empty when the
    /// authenticator answered with the status byte alone).
    pub fn call(
        &mut self,
        command: u8,
        params: Option<ciborium::Value>,
        control: &mut Control<'_>,
    ) -> Result<ciborium::Value, CtapError> {
        self.channel.cbor(command, params, control)
    }
}
