//! Batch 4: account notices the server pushes at the user — the
//! new-login alert (`updateUnconfirmedSession`), service notification
//! popups (`updateServiceNotification`) and terms of service
//! (`updateTermsOfService`) — and Batch 6: the two-step verification
//! recovery/reset flow state. Pure state and text, no UI.
use super::*;
use crate::telegram::envelope::{ResetPasswordOutcome, TermsOfService, UnconfirmedLogin};

/// One unconfirmed login resolved from `getActiveSessions`
/// (`session.is_unconfirmed`), the only place its id is available.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnconfirmedEntry {
    pub id: i64,
    pub device: String,
    pub location: String,
}

/// What the user decided on the new-login alert, shown once as the
/// follow-up (tdesktop `ShowAuthToast`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginReview {
    /// "Yes, it's me": "New Login Allowed".
    Allowed,
    /// "No, it's not me!": "New Login Prevented" with the terminated
    /// attempts (`location (device)` lines).
    Prevented { places: Vec<String> },
}

/// A server popup (`updateServiceNotification`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceNotice {
    pub kind: String,
    pub text: String,
}

impl ServiceNotice {
    /// tdesktop `IsForceLogoutNotification`: the auth key was dropped,
    /// the only action is to log out.
    pub fn is_force_logout(&self) -> bool {
        self.kind.starts_with("AUTH_KEY_DROP_")
    }
}

#[derive(Debug, Clone, Default)]
pub struct AccountNotices {
    /// The first unconfirmed login from the last update (`None` when
    /// every login is confirmed) and the total count.
    pub unconfirmed: Option<UnconfirmedLogin>,
    pub unconfirmed_count: i32,
    /// Sessions behind the alert, resolved from `getActiveSessions`.
    pub unconfirmed_entries: Vec<UnconfirmedEntry>,
    /// `confirmSession` / `terminateSession` requests still in flight.
    pub review_pending: usize,
    pub review_confirmed: bool,
    pub review_error: Option<String>,
    /// The follow-up after a finished review; the UI takes it once.
    pub review_outcome: Option<LoginReview>,
    /// Popups in arrival order; the UI shows the front one.
    pub service: VecDeque<ServiceNotice>,
    /// Terms the user must accept, and the accept round-trip.
    pub terms: Option<TermsOfService>,
    pub terms_in_flight: bool,
    pub terms_error: Option<String>,
}

/// Where the two-step recovery/reset flow is, as the dialog needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TwofaFlow {
    /// `requestPasswordRecovery` answered: the emailed code is awaited.
    pub recovery_code_sent_to: Option<String>,
    /// `setLoginEmailAddress` answered: the emailed code is awaited.
    pub login_email_code_sent_to: Option<String>,
    /// A finished step the dialog reports once.
    pub notice: Option<TwofaNotice>,
    /// The cached password state is out of date (cancel reset, login
    /// email changed): the driver refetches it.
    pub refetch: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TwofaNotice {
    /// "Two-step verification was disabled."
    PasswordRemoved,
    /// "Your cloud password was updated."
    PasswordRecovered,
    /// A reset was requested; the password can be reset after this date.
    ResetPending { reset_date: i32 },
    /// A recent reset was declined; retry after this date.
    ResetDeclined { retry_date: i32 },
    /// The pending reset was cancelled.
    ResetCancelled,
    /// "Your login email has been set successfully."
    LoginEmailChanged,
    /// The recovery email was confirmed.
    RecoveryEmailConfirmed,
}

impl Session {
    pub(crate) fn apply_unconfirmed_session(
        &mut self,
        session: Option<UnconfirmedLogin>,
        count: i32,
    ) {
        self.notices.unconfirmed = session;
        self.notices.unconfirmed_count = count;
        if count == 0 {
            self.notices.unconfirmed_entries.clear();
        } else {
            // The update carries no session id: refetch the list; its
            // answer fills `unconfirmed_entries`.
            self.sessions_stale = true;
        }
    }

    /// `getActiveSessions` answered: resolve the sessions behind the
    /// alert from `is_unconfirmed`.
    pub(crate) fn resolve_unconfirmed_entries(&mut self, sessions: &[ParsedSession]) {
        self.notices.unconfirmed_entries = sessions
            .iter()
            .filter(|session| session.is_unconfirmed)
            .map(|session| UnconfirmedEntry {
                id: session.id,
                device: session.device_model.clone(),
                location: session.location.clone(),
            })
            .collect();
    }

    pub(crate) fn apply_service_notification(&mut self, kind: String, text: String) {
        // tdesktop ignores these (`IsWithdrawalNotification`); an empty
        // popup has nothing to show.
        if kind.starts_with("API_WITHDRAWAL_FEATURE_DISABLED_") || text.trim().is_empty() {
            return;
        }
        self.notices.service.push_back(ServiceNotice { kind, text });
    }

    /// The front popup is dismissed.
    pub fn dismiss_service_notice(&mut self) {
        self.notices.service.pop_front();
    }

    /// One `confirmSession` / `terminateSession` finished.
    pub(crate) fn finish_login_review(&mut self, confirmed: bool, error: Option<String>) {
        self.notices.review_pending = self.notices.review_pending.saturating_sub(1);
        if let Some(error) = error {
            self.notices.review_error = Some(error);
        }
        if self.notices.review_pending > 0 {
            return;
        }
        if self.notices.review_error.is_none() {
            let places = if confirmed {
                Vec::new()
            } else {
                self.notices
                    .unconfirmed_entries
                    .iter()
                    .map(|entry| unconfirmed_place(&entry.device, &entry.location))
                    .collect()
            };
            self.notices.review_outcome = Some(if confirmed {
                LoginReview::Allowed
            } else {
                LoginReview::Prevented { places }
            });
            self.notices.unconfirmed = None;
            self.notices.unconfirmed_count = 0;
            self.notices.unconfirmed_entries.clear();
        }
        self.sessions_stale = true;
    }

    pub(crate) fn apply_email_code_info(&mut self, op: PasswordOp, pattern: String) {
        match op {
            PasswordOp::RequestRecoveryCode => {
                self.twofa_flow.recovery_code_sent_to = Some(pattern);
            }
            PasswordOp::SetLoginEmail => {
                self.twofa_flow.login_email_code_sent_to = Some(pattern);
            }
            _ => {}
        }
    }

    pub(crate) fn apply_reset_password_result(&mut self, outcome: ResetPasswordOutcome) {
        self.twofa_flow.recovery_code_sent_to = None;
        self.twofa_flow.notice = Some(match outcome {
            ResetPasswordOutcome::Ok => TwofaNotice::PasswordRemoved,
            ResetPasswordOutcome::Pending { reset_date } => TwofaNotice::ResetPending { reset_date },
            ResetPasswordOutcome::Declined { retry_date } => TwofaNotice::ResetDeclined { retry_date },
        });
        // The pending date (or the removed password) lives in the state.
        self.twofa_flow.refetch = true;
    }

    /// A `PasswordStateOp` answered `passwordState`.
    pub(crate) fn apply_password_state_op(&mut self, op: PasswordOp) {
        match op {
            PasswordOp::RecoverPassword => {
                self.twofa_flow.recovery_code_sent_to = None;
                let still_on = self.password_state.as_ref().is_some_and(|s| s.has_password);
                self.twofa_flow.notice = Some(if still_on {
                    TwofaNotice::PasswordRecovered
                } else {
                    TwofaNotice::PasswordRemoved
                });
            }
            PasswordOp::CheckEmailCode => {
                self.twofa_flow.notice = Some(TwofaNotice::RecoveryEmailConfirmed);
            }
            _ => {}
        }
    }

    /// A `PasswordStateOp` answered `ok`.
    pub(crate) fn apply_password_op_ok(&mut self, op: PasswordOp) {
        self.password_state_loading = false;
        self.password_op_error = None;
        match op {
            PasswordOp::CancelPasswordReset => {
                self.twofa_flow.notice = Some(TwofaNotice::ResetCancelled);
                self.twofa_flow.refetch = true;
            }
            PasswordOp::CheckLoginEmailCode => {
                self.twofa_flow.login_email_code_sent_to = None;
                self.twofa_flow.notice = Some(TwofaNotice::LoginEmailChanged);
                self.twofa_flow.refetch = true;
            }
            _ => {}
        }
    }
}

/// `location (device)` as tdesktop lists the prevented attempts.
pub fn unconfirmed_place(device: &str, location: &str) -> String {
    match (location.is_empty(), device.is_empty()) {
        (false, false) => format!("{location} ({device})"),
        (false, true) => location.to_string(),
        (true, false) => device.to_string(),
        (true, true) => String::new(),
    }
}

/// tdesktop `FormatUnconfirmedAuthMessage` (`lng_unconfirmed_auth_*`).
pub fn unconfirmed_login_message(entries: &[UnconfirmedEntry]) -> String {
    match entries {
        [] => String::new(),
        [only] => format!(
            "We detected a new login to your account from {}, {}. Is it you?",
            only.device, only.location
        ),
        [first, rest @ ..] => {
            let count = entries.len();
            let common = rest
                .iter()
                .all(|entry| entry.location == first.location)
                .then_some(first.location.as_str())
                .filter(|location| !location.is_empty());
            match common {
                Some(country) => {
                    format!("We detected new {count} logins to your account from {country}. Is it you?")
                }
                None => format!("We detected new {count} logins to your account. Is it you?"),
            }
        }
    }
}

/// tdesktop `lng_unconfirmed_auth_denied_single/multiple`.
pub fn prevented_login_message(places: &[String]) -> String {
    match places {
        [only] => format!("We have terminated the login attempt from {only}."),
        _ => {
            let mut text = String::from("We have terminated the login attempts from:");
            for place in places.iter().take(10).filter(|place| !place.is_empty()) {
                text.push_str("\n\u{2022} ");
                text.push_str(place);
            }
            text
        }
    }
}

/// tdesktop `Ui::FormatResetCloudPasswordIn` for the reset countdown:
/// "N days", "N hours", "N minutes" or "N seconds".
pub fn format_reset_duration(secs: i64) -> String {
    let secs = secs.max(1);
    let (value, unit) = if secs >= 86_400 {
        (secs / 86_400, "day")
    } else if secs >= 3_600 {
        (secs / 3_600, "hour")
    } else if secs >= 60 {
        (secs / 60, "minute")
    } else {
        (secs, "second")
    };
    format!("{value} {unit}{}", if value == 1 { "" } else { "s" })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: i64, device: &str, location: &str) -> UnconfirmedEntry {
        UnconfirmedEntry {
            id,
            device: device.into(),
            location: location.into(),
        }
    }

    #[test]
    fn single_login_message_names_device_and_place() {
        assert_eq!(
            unconfirmed_login_message(&[entry(1, "Pixel 9", "Berlin, Germany")]),
            "We detected a new login to your account from Pixel 9, Berlin, Germany. Is it you?"
        );
    }

    #[test]
    fn several_logins_share_a_country_only_when_they_agree() {
        let same = [entry(1, "a", "Germany"), entry(2, "b", "Germany")];
        assert_eq!(
            unconfirmed_login_message(&same),
            "We detected new 2 logins to your account from Germany. Is it you?"
        );
        let mixed = [entry(1, "a", "Germany"), entry(2, "b", "France")];
        assert_eq!(
            unconfirmed_login_message(&mixed),
            "We detected new 2 logins to your account. Is it you?"
        );
    }

    #[test]
    fn prevented_message_lists_places() {
        assert_eq!(
            prevented_login_message(&["Berlin (Pixel)".into()]),
            "We have terminated the login attempt from Berlin (Pixel)."
        );
        assert_eq!(
            prevented_login_message(&["A".into(), "B".into()]),
            "We have terminated the login attempts from:\n\u{2022} A\n\u{2022} B"
        );
    }

    #[test]
    fn reset_duration_uses_the_largest_unit() {
        assert_eq!(format_reset_duration(7 * 86_400), "7 days");
        assert_eq!(format_reset_duration(86_400), "1 day");
        assert_eq!(format_reset_duration(3 * 3_600 + 5), "3 hours");
        assert_eq!(format_reset_duration(90), "1 minute");
        assert_eq!(format_reset_duration(0), "1 second");
    }

    #[test]
    fn force_logout_notices_are_detected() {
        let drop = ServiceNotice {
            kind: "AUTH_KEY_DROP_DUPLICATE".into(),
            text: "x".into(),
        };
        assert!(drop.is_force_logout());
        let plain = ServiceNotice {
            kind: String::new(),
            text: "x".into(),
        };
        assert!(!plain.is_force_logout());
    }
}
