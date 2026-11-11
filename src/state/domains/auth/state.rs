//! Sign-in errors, countries, the phone-number change and two-step verification: the `auth` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;

pub struct AuthState {
    /// Last classified error for phone / code / password submit. Never a secret.
    pub last_auth_error: Option<AuthRequestError>,
    /// `getCountries` rows for the sign-in picker and the country boxes:
    /// never asked, in flight, answered or failed (a box then says so and
    /// offers Retry).
    pub countries: crate::state::fetch::Fetch<Vec<crate::phone::Country>>,
    /// Uppercase ISO code from `getCountryCode`: the default country guess.
    pub guessed_country_iso: Option<String>,
    /// Slice A8: the target number a change-number code was sent to
    /// (`authenticationCodeInfo` answer) — drives the code-entry step of
    /// the change-number flow. The code itself is never stored (the A2
    /// rule: secrets ride the request JSON only).
    pub change_number_phone: Option<String>,
    /// Slice A8: server-specified timeout (seconds) before a resend is
    /// allowed, from the same `authenticationCodeInfo` answer.
    pub change_number_timeout: Option<i32>,
    /// Slice A8: a `sendPhoneNumberCode` / `resendPhoneNumberCode` round
    /// trip is in flight.
    pub change_number_loading: bool,
    /// Slice A8: a `checkPhoneNumberCode` round trip is in flight.
    pub change_number_checking: bool,
    /// Slice A8: honest one-line failure of the last change-number op
    /// (classified, never the native message). Cleared on the next
    /// attempt and on success.
    pub change_number_error: Option<String>,
    /// Batch 6: two-step recovery / reset / login-email flow state.
    pub twofa_flow: TwofaFlow,
    /// Slice A2: cached `getPasswordState` / `setPassword` /
    /// `setRecoveryEmailAddress` answer; drives the two-step
    /// verification overlay. Replaced only by our own
    /// `PasswordStateOp` answers — never mutated optimistically.
    pub password_state: Option<PasswordState>,
    /// Slice A2: the 2FA management round trip in flight (fetch or
    /// mutation) under its request; the overlay shows progress and
    /// disables submits. Only that request's answer or error ends it.
    pub password_op: PasswordFlight,
    /// Slice A2: honest one-line failure of the last 2FA management
    /// request (TDLib's actual error, classified — never a fake
    /// success). Cleared on the next attempt and on success.
    pub password_op_error: Option<String>,
    /// The `logOut` sent and not answered yet (`Session::logout_sent`).
    pub logout_request: Option<RequestId>,
    /// TDLib refused the `logOut`: the account stays signed in, and the UI
    /// says so once (drained by the poll).
    pub logout_refused: Option<TdError>,
}

impl AuthState {
    pub(crate) fn new() -> Self {
        Self {
            last_auth_error: None,
            countries: crate::state::fetch::Fetch::Idle,
            guessed_country_iso: None,
            change_number_phone: None,
            change_number_timeout: None,
            change_number_loading: false,
            change_number_checking: false,
            change_number_error: None,
            twofa_flow: TwofaFlow::default(),
            password_state: None,
            password_op: PasswordFlight::Idle,
            password_op_error: None,
            logout_request: None,
            logout_refused: None,
        }
    }
}

/// The 2FA management request in flight, if any.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PasswordFlight {
    #[default]
    Idle,
    InFlight(RequestId),
}

impl AuthState {
    /// A 2FA management round trip is out.
    pub fn password_state_loading(&self) -> bool {
        matches!(self.password_op, PasswordFlight::InFlight(_))
    }

    /// `request` answered (or failed): ends the round trip when it is the
    /// one in flight. An older request's late answer leaves a newer one
    /// waiting. Returns whether it was the one.
    pub fn settle_password_op(&mut self, request: Option<RequestId>) -> bool {
        match (self.password_op, request) {
            (PasswordFlight::InFlight(waiting), Some(id)) if waiting == id => {
                self.password_op = PasswordFlight::Idle;
                true
            }
            _ => false,
        }
    }
}
