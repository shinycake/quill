//! One WebAuthn ceremony with a USB security key, on a worker thread.
//! The flow is Telegram Desktop's libfido2 path
//! (`webauthn/webauthn_common.cpp` `AttemptRegister` / `AttemptLogin`):
//! try without a PIN; when the key wants one (or the PIN was wrong) ask
//! for it and try again; map the key's errors to what the user is told.
//! One difference: tdesktop checks that a key is present before it starts
//! (its first sheet's "Use a security key" button); Quill has no first
//! sheet yet, so the worker waits for a key to be plugged in, up to the
//! request's timeout.
#![deny(clippy::indexing_slicing)]

use super::ctap2::commands::{get_assertion, make_credential, none_attestation_object};
use super::ctap2::pin::{
    PERMISSION_GET_ASSERTION, PERMISSION_MAKE_CREDENTIAL, PinToken, pin_token,
};
use super::ctap2::{Authenticator, Control, CtapError, HidDevice, HidError, status};
use super::{Assertion, CeremonyError, CreationOptions, Registration, RequestOptions};
use sha2_next::{Digest, Sha256};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError, channel};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

/// How often the worker looks for a key while none is plugged in.
pub const DEVICE_POLL: Duration = Duration::from_millis(500);

/// Where the worker finds security keys.
pub trait DeviceSource: Send {
    fn open_all(&mut self) -> Vec<Box<dyn HidDevice>>;
}

/// The USB keys of this computer (`ctap2::hid`).
pub struct UsbKeys;

impl DeviceSource for UsbKeys {
    fn open_all(&mut self) -> Vec<Box<dyn HidDevice>> {
        super::ctap2::hid::open_all()
    }
}

/// What the worker tells the dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CeremonyEvent {
    /// Ask for the key's PIN (tdesktop `ShowPinBox`); `invalid` after a
    /// wrong one ("Incorrect PIN. Please try again.").
    NeedPin {
        invalid: bool,
    },
    Registered(Registration),
    SignedIn(Assertion),
    Failed(CeremonyError),
}

/// What a backend runs when the dialog cancels, besides raising the
/// cancel flag (Windows: `WebAuthNCancelCurrentOperation`).
pub(super) type OnCancel = Arc<dyn Fn() + Send + Sync>;

/// The dialog's handle on a running ceremony. Dropping it cancels.
pub struct Ceremony {
    events: Receiver<CeremonyEvent>,
    pins: Sender<Zeroizing<String>>,
    cancel: Arc<AtomicBool>,
    on_cancel: Option<OnCancel>,
    finished: bool,
}

/// The worker's side of the channels.
struct Link {
    events: Sender<CeremonyEvent>,
    pins: Receiver<Zeroizing<String>>,
    cancel: Arc<AtomicBool>,
}

impl Ceremony {
    fn spawn(work: impl FnOnce(&Link) -> CeremonyEvent + Send + 'static) -> Self {
        Self::spawn_with(work, None)
    }

    /// A ceremony run by the system's own dialog (`webauthn.dll`): one
    /// blocking call on the worker, no PIN round trips; `on_cancel` stops
    /// the call.
    #[cfg(any(windows, test))]
    pub(super) fn spawn_system(
        work: impl FnOnce() -> CeremonyEvent + Send + 'static,
        on_cancel: OnCancel,
    ) -> Self {
        Self::spawn_with(move |_| work(), Some(on_cancel))
    }

    fn spawn_with(
        work: impl FnOnce(&Link) -> CeremonyEvent + Send + 'static,
        on_cancel: Option<OnCancel>,
    ) -> Self {
        let (event_tx, events) = channel();
        let (pins, pin_rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let link = Link {
            events: event_tx.clone(),
            pins: pin_rx,
            cancel: cancel.clone(),
        };
        let spawned = crate::thread_guard::spawn_guarded("quill-passkey", move || {
            let last = work(&link);
            let _ = link.events.send(last);
        });
        if spawned.is_err() {
            let _ = event_tx.send(CeremonyEvent::Failed(CeremonyError::Other(
                "could not start the worker".into(),
            )));
        }
        Self {
            events,
            pins,
            cancel,
            on_cancel,
            finished: false,
        }
    }

    /// Create a passkey on a security key.
    pub fn register(options: CreationOptions, mut source: Box<dyn DeviceSource>) -> Self {
        Self::spawn(
            move |link| match run_register(&options, source.as_mut(), link) {
                Ok(registration) => CeremonyEvent::Registered(registration),
                Err(error) => CeremonyEvent::Failed(error),
            },
        )
    }

    /// Sign in with a passkey on a security key.
    pub fn login(options: RequestOptions, mut source: Box<dyn DeviceSource>) -> Self {
        Self::spawn(
            move |link| match run_login(&options, source.as_mut(), link) {
                Ok(assertion) => CeremonyEvent::SignedIn(assertion),
                Err(error) => CeremonyEvent::Failed(error),
            },
        )
    }

    /// The events since the last look. A worker that went away without a
    /// last word (a caught panic) reads as a failure, once.
    pub fn poll(&mut self) -> Vec<CeremonyEvent> {
        let mut out = Vec::new();
        loop {
            match self.events.try_recv() {
                Ok(event) => {
                    if matches!(
                        event,
                        CeremonyEvent::Registered(_)
                            | CeremonyEvent::SignedIn(_)
                            | CeremonyEvent::Failed(_)
                    ) {
                        self.finished = true;
                    }
                    out.push(event);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if !self.finished {
                        self.finished = true;
                        out.push(CeremonyEvent::Failed(CeremonyError::Other(
                            "the worker stopped".into(),
                        )));
                    }
                    break;
                }
            }
        }
        out
    }

    /// The PIN the user typed, for the next attempt.
    pub fn submit_pin(&self, pin: Zeroizing<String>) {
        let _ = self.pins.send(pin);
    }

    /// Stop: the key gets CTAPHID_CANCEL (Windows: the system dialog is
    /// cancelled) and the worker ends.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        if let Some(on_cancel) = &self.on_cancel {
            on_cancel();
        }
    }
}

impl Drop for Ceremony {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn sha256(data: &[u8]) -> Vec<u8> {
    Sha256::digest(data).to_vec()
}

/// What one attempt ended with (tdesktop `Outcome`).
enum Attempt<T> {
    Done(T),
    NeedPin,
    PinInvalid,
    Failed(CeremonyError),
}

/// tdesktop `MapError`.
fn map_error<T>(error: CtapError) -> Attempt<T> {
    match error {
        CtapError::Status(status::PIN_REQUIRED) => Attempt::NeedPin,
        CtapError::Status(status::PIN_INVALID) => Attempt::PinInvalid,
        CtapError::Status(status::PIN_BLOCKED | status::PIN_AUTH_BLOCKED) => {
            Attempt::Failed(CeremonyError::PinBlocked)
        }
        CtapError::Status(status::NO_CREDENTIALS) => Attempt::Failed(CeremonyError::NoCredentials),
        CtapError::Hid(HidError::Cancelled) => Attempt::Failed(CeremonyError::Cancelled),
        other => Attempt::Failed(CeremonyError::Other(other.to_string())),
    }
}

impl Link {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    /// The keys present, waiting for one to be plugged in until
    /// `deadline`.
    fn devices(
        &self,
        source: &mut dyn DeviceSource,
        deadline: Instant,
    ) -> Result<Vec<Box<dyn HidDevice>>, CeremonyError> {
        loop {
            if self.cancelled() {
                return Err(CeremonyError::Cancelled);
            }
            let devices = source.open_all();
            if !devices.is_empty() {
                return Ok(devices);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(CeremonyError::NoDevice);
            }
            std::thread::sleep(left.min(DEVICE_POLL));
        }
    }

    /// Ask for the PIN and wait for it (no deadline: the dialog waits for
    /// the user, as tdesktop's PIN box does).
    fn ask_pin(&self, invalid: bool) -> Result<Zeroizing<String>, CeremonyError> {
        let _ = self.events.send(CeremonyEvent::NeedPin { invalid });
        loop {
            if self.cancelled() {
                return Err(CeremonyError::Cancelled);
            }
            match self.pins.recv_timeout(Duration::from_millis(100)) {
                Ok(pin) => return Ok(pin),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Err(CeremonyError::Cancelled),
            }
        }
    }

    /// Attempts until one finishes, asking for a PIN between them.
    fn run<T>(
        &self,
        mut attempt: impl FnMut(Option<&str>) -> Attempt<T>,
    ) -> Result<T, CeremonyError> {
        let mut pin: Option<Zeroizing<String>> = None;
        loop {
            match attempt(pin.as_deref().map(String::as_str)) {
                Attempt::Done(value) => return Ok(value),
                Attempt::NeedPin => pin = Some(self.ask_pin(false)?),
                Attempt::PinInvalid => pin = Some(self.ask_pin(true)?),
                Attempt::Failed(error) => {
                    if self.cancelled() {
                        return Err(CeremonyError::Cancelled);
                    }
                    return Err(error);
                }
            }
        }
    }
}

/// A PIN token for `permission`, when the user gave a PIN.
fn token_for(
    key: &mut Authenticator,
    pin: Option<&str>,
    permission: u8,
    rp_id: &str,
    control: &mut Control<'_>,
) -> Result<Option<PinToken>, CtapError> {
    let Some(pin) = pin else {
        return Ok(None);
    };
    pin_token(key, pin, permission, rp_id, &mut rand::rngs::OsRng, control).map(Some)
}

/// tdesktop `RunRegisterCeremony`: the first key that opens gets
/// makeCredential.
fn run_register(
    options: &CreationOptions,
    source: &mut dyn DeviceSource,
    link: &Link,
) -> Result<Registration, CeremonyError> {
    let client_data_json = options.client_data_json();
    let hash = sha256(client_data_json.as_bytes());
    let timeout = Duration::from_millis(u64::from(options.timeout_ms));
    link.run(|pin| {
        let deadline = Instant::now() + timeout;
        let devices = match link.devices(source, deadline) {
            Ok(devices) => devices,
            Err(error) => return Attempt::Failed(error),
        };
        let mut ignore = |_: u8| {};
        let mut control = Control {
            cancel: &link.cancel,
            deadline,
            on_keepalive: &mut ignore,
        };
        let mut key = None;
        for device in devices {
            match Authenticator::open(device, &mut control) {
                Ok(opened) => {
                    key = Some(opened);
                    break;
                }
                Err(error) => super::log(&format!("could not open the key: {error}")),
            }
        }
        let Some(mut key) = key else {
            return Attempt::Failed(CeremonyError::Other("no key opened".into()));
        };
        if !key.info().is_fido2() {
            return Attempt::Failed(CeremonyError::Other("not a CTAP2 key".into()));
        }
        let made = token_for(
            &mut key,
            pin,
            PERMISSION_MAKE_CREDENTIAL,
            &options.rp_id,
            &mut control,
        )
        .and_then(|token| make_credential(&mut key, options, &hash, token.as_ref(), &mut control));
        match made {
            Ok(made) => Attempt::Done(Registration {
                credential_id: made.credential_id,
                attestation_object: none_attestation_object(&made.auth_data),
                client_data_json: client_data_json.clone(),
            }),
            Err(error) => {
                super::log(&format!("credential failed: {error}"));
                map_error(error)
            }
        }
    })
}

/// tdesktop `RunLoginCeremony`: every key in turn until one signs; a PIN
/// question or a blocked PIN ends the round at once.
fn run_login(
    options: &RequestOptions,
    source: &mut dyn DeviceSource,
    link: &Link,
) -> Result<Assertion, CeremonyError> {
    let client_data_json = options.client_data_json();
    let hash = sha256(client_data_json.as_bytes());
    let timeout = Duration::from_millis(u64::from(options.timeout_ms));
    link.run(|pin| {
        let deadline = Instant::now() + timeout;
        let devices = match link.devices(source, deadline) {
            Ok(devices) => devices,
            Err(error) => return Attempt::Failed(error),
        };
        let mut last = Attempt::Failed(CeremonyError::Other("no key opened".into()));
        for device in devices {
            let mut ignore = |_: u8| {};
            let mut control = Control {
                cancel: &link.cancel,
                deadline,
                on_keepalive: &mut ignore,
            };
            let got = Authenticator::open(device, &mut control).and_then(|mut key| {
                let token = token_for(
                    &mut key,
                    pin,
                    PERMISSION_GET_ASSERTION,
                    &options.rp_id,
                    &mut control,
                )?;
                get_assertion(&mut key, options, &hash, token.as_ref(), &mut control)
            });
            match got {
                Ok(got) => {
                    return Attempt::Done(Assertion {
                        credential_id: got.credential_id,
                        authenticator_data: got.auth_data,
                        signature: got.signature,
                        user_handle: got.user_handle,
                        client_data_json: client_data_json.clone(),
                    });
                }
                Err(error) => {
                    super::log(&format!("assertion failed: {error}"));
                    match map_error(error) {
                        Attempt::NeedPin => return Attempt::NeedPin,
                        Attempt::PinInvalid => return Attempt::PinInvalid,
                        Attempt::Failed(CeremonyError::PinBlocked) => {
                            return Attempt::Failed(CeremonyError::PinBlocked);
                        }
                        Attempt::Failed(CeremonyError::Cancelled) => {
                            return Attempt::Failed(CeremonyError::Cancelled);
                        }
                        other => last = other,
                    }
                }
            }
        }
        last
    })
}

#[cfg(test)]
#[path = "ceremony_tests.rs"]
mod tests;
