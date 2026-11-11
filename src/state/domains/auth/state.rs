//! Sign-in errors, countries, the phone-number change and two-step verification: the `auth` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;

/// The 2FA password step's recovery e-mail: one stage, moved by the
/// answer to the request that asked (`requestAuthenticationPasswordRecovery`),
/// so the screen asks for the code only once the server sent it. A refused
/// request, or auth leaving the password step, goes back to `Idle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PasswordRecovery {
    #[default]
    Idle,
    Requested(RequestId),
    CodeSent,
}

impl AuthState {
    /// The user left recovery-code entry (Back), or the request was refused
    /// or the auth step ended: back to password entry.
    pub fn cancel_password_recovery(&mut self) {
        self.password_recovery = PasswordRecovery::Idle;
    }
}

pub struct AuthState {
    /// See [`PasswordRecovery`].
    pub password_recovery: PasswordRecovery,
    /// Last classified error for phone / code / password submit. Never a secret.
    pub last_auth_error: Option<AuthRequestError>,
    /// `getCountries` rows for the sign-in picker and the country boxes:
    /// never asked, in flight, answered or failed (a box then says so and
    /// offers Retry).
    pub countries: crate::state::fetch::Fetch<Vec<crate::phone::Country>>,
    /// Uppercase ISO code from `getCountryCode`: the default country guess.
    pub guessed_country_iso: Option<String>,
    /// Slice A8: the while-authorized change-number flow: the target
    /// number the code was sent to, the resend timeout, the round trip in
    /// flight and the honest one-line failure of the last operation.
    pub change_number: ChangeNumber,
    /// Batch 6: two-step recovery / reset / login-email flow state.
    pub twofa_flow: TwofaFlow,
    /// Slice A2: cached `getPasswordState` / `setPassword` /
    /// `setRecoveryEmailAddress` answer; drives the two-step
    /// verification overlay. Replaced only by our own
    /// `PasswordStateOp` answers — never mutated optimistically.
    pub password_state: PasswordCache,
    /// Slice A2: the 2FA management round trip (fetch or mutation) in
    /// flight under its request, or the honest one-line failure of the last
    /// one; the overlay shows progress and disables submits. Only that
    /// request's answer or error ends it.
    pub password_op: PasswordFlight,
    /// The `logOut` sent and not answered yet (`Session::logout_sent`).
    pub logout_request: Option<RequestId>,
    /// TDLib refused the `logOut`: the account stays signed in, and the UI
    /// says so once (drained by the poll).
    pub logout_refused: Option<TdError>,
}

impl AuthState {
    pub(crate) fn new() -> Self {
        Self {
            password_recovery: PasswordRecovery::Idle,
            last_auth_error: None,
            countries: crate::state::fetch::Fetch::Idle,
            guessed_country_iso: None,
            change_number: ChangeNumber::Idle,
            twofa_flow: TwofaFlow::default(),
            password_state: PasswordCache::Unknown,
            password_op: PasswordFlight::Idle,
            logout_request: None,
            logout_refused: None,
        }
    }
}

/// The cached `passwordState` and whether it can be trusted: a reset, a
/// cancelled reset or a changed login email leave it out of date, and it is
/// asked again under its own request. Only that request's answer ends the
/// refetch; a refusal keeps what is shown and does not ask again by itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum PasswordCache {
    /// Never answered.
    #[default]
    Unknown,
    Fresh(PasswordState),
    /// Something changed after the answer: it is shown until the refetch
    /// goes out.
    Outdated(PasswordState),
    /// The refetch is out under `request`.
    Refetching {
        state: PasswordState,
        request: RequestId,
    },
}

impl PasswordCache {
    pub fn get(&self) -> Option<&PasswordState> {
        match self {
            Self::Unknown => None,
            Self::Fresh(state) | Self::Outdated(state) | Self::Refetching { state, .. } => {
                Some(state)
            }
        }
    }

    /// A refetch is due and not yet out.
    pub fn is_outdated(&self) -> bool {
        matches!(self, Self::Outdated(_))
    }

    /// The answer of `request` replaces the state. A change marked after
    /// that request went out keeps its refetch.
    pub fn answered(&mut self, request: Option<RequestId>, state: PasswordState) {
        *self = match self {
            Self::Outdated(_) => Self::Outdated(state),
            Self::Refetching { request: out, .. } if Some(*out) != request => Self::Refetching {
                state,
                request: *out,
            },
            _ => Self::Fresh(state),
        };
    }

    /// The answer is out of date: it is fetched again.
    pub fn mark_outdated(&mut self) {
        *self = match std::mem::take(self) {
            Self::Fresh(state) | Self::Refetching { state, .. } => Self::Outdated(state),
            other => other,
        };
    }

    /// The refetch went out under `request`.
    pub fn refetching(&mut self, request: RequestId) {
        *self = match std::mem::take(self) {
            Self::Outdated(state) => Self::Refetching { state, request },
            other => other,
        };
    }

    /// `request` was refused: the state shown stays and is not asked again.
    pub fn refetch_failed(&mut self, request: Option<RequestId>) {
        *self = match std::mem::take(self) {
            Self::Refetching {
                state,
                request: out,
            } if Some(out) == request => Self::Fresh(state),
            other => other,
        };
    }
}

/// The 2FA management request: none, in flight, or the failure line of the
/// last one (TDLib's actual error, classified, never a fake success). The
/// next attempt, a success and closing the box clear the line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum PasswordFlight {
    #[default]
    Idle,
    InFlight(RequestId),
    Failed(String),
}

impl AuthState {
    /// A 2FA management round trip is out.
    pub fn password_state_loading(&self) -> bool {
        matches!(self.password_op, PasswordFlight::InFlight(_))
    }

    /// The failure line of the last 2FA request.
    pub fn password_op_error(&self) -> Option<&str> {
        match &self.password_op {
            PasswordFlight::Failed(line) => Some(line),
            _ => None,
        }
    }

    /// The last failure line belongs to the box it was shown in. A round
    /// trip in flight keeps its place.
    pub fn clear_password_error(&mut self) {
        if matches!(self.password_op, PasswordFlight::Failed(_)) {
            self.password_op = PasswordFlight::Idle;
        }
    }

    /// `request` failed with `line`: ends the round trip when it is the one
    /// in flight. Returns whether it was.
    pub fn fail_password_op(&mut self, request: Option<RequestId>, line: String) -> bool {
        self.password_state.refetch_failed(request);
        if self.settle_password_op(request) {
            self.password_op = PasswordFlight::Failed(line);
            return true;
        }
        false
    }

    /// `request` answered (or failed): ends the round trip when it is the
    /// one in flight. An older request's late answer leaves a newer one
    /// waiting. Returns whether it was the one.
    pub fn settle_password_op(&mut self, request: Option<RequestId>) -> bool {
        match (&self.password_op, request) {
            (PasswordFlight::InFlight(waiting), Some(id)) if *waiting == id => {
                self.password_op = PasswordFlight::Idle;
                true
            }
            _ => false,
        }
    }
}
