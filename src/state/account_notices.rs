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

/// One session of a review: it is answered by its own request, which is
/// unknown until the send goes out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewItem {
    pub entry: UnconfirmedEntry,
    pub stage: ReviewItemStage,
}

/// Where one session of a review is. `ToSend` leaves when the send goes out
/// (`InFlight`) or fails ([`Session::login_review_unsent`] settles the
/// item); `InFlight` leaves with its own request's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewItemStage {
    ToSend,
    InFlight(RequestId),
}

/// The new-login review ("Yes, it's me" / "No, it's not me!"): one
/// `confirmSession` / `terminateSession` per unconfirmed session. A session
/// leaves the alert when its own request succeeds, so a retry after a
/// partial failure sends only the ones still unreviewed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ReviewStage {
    #[default]
    Idle,
    /// Requests are out; `items` are the sessions still waiting for their
    /// own answer, `done` the sessions already reviewed, `error` the first
    /// failure so far.
    Reviewing {
        confirmed: bool,
        items: Vec<ReviewItem>,
        done: Vec<UnconfirmedEntry>,
        error: Option<String>,
    },
    /// Every request answered and one failed: the alert stays with the
    /// failure line until the next press (which starts `Reviewing` again).
    Failed {
        error: String,
        done: Vec<UnconfirmedEntry>,
    },
    /// Every request succeeded: the follow-up waits for the UI, which takes
    /// it once ([`ReviewStage::take_outcome`]).
    Reviewed(LoginReview),
}

impl ReviewStage {
    /// Requests are still in flight.
    pub fn busy(&self) -> bool {
        matches!(self, Self::Reviewing { .. })
    }

    /// The failure line the alert draws.
    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Reviewing { error, .. } => error.as_deref(),
            Self::Failed { error, .. } => Some(error),
            Self::Idle | Self::Reviewed(_) => None,
        }
    }

    /// The sessions reviewed so far, kept when the press is retried.
    pub fn done(&self) -> Vec<UnconfirmedEntry> {
        match self {
            Self::Reviewing { done, .. } | Self::Failed { done, .. } => done.clone(),
            Self::Idle | Self::Reviewed(_) => Vec::new(),
        }
    }

    /// The follow-up of a finished review, until the UI takes it.
    pub fn outcome(&self) -> Option<&LoginReview> {
        match self {
            Self::Reviewed(outcome) => Some(outcome),
            _ => None,
        }
    }

    /// The follow-up of a finished review, once.
    pub fn take_outcome(&mut self) -> Option<LoginReview> {
        match std::mem::take(self) {
            Self::Reviewed(outcome) => Some(outcome),
            other => {
                *self = other;
                None
            }
        }
    }
}

/// The terms of service the user must accept, and the accept round trip.
/// The failure and the wait belong to the terms they were asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum TermsState {
    #[default]
    None,
    Offered(TermsOfService),
    /// `acceptTermsOfService` is out for these terms.
    Accepting(TermsOfService),
    /// The send or the request failed; the box draws the line.
    Failed {
        terms: TermsOfService,
        error: String,
    },
}

impl TermsState {
    /// The server pushed `terms`. The same terms already being accepted (or
    /// failed) keep that stage: the accept in flight is not undone.
    pub fn offer(&mut self, terms: TermsOfService) {
        if self.terms().map(|t| t.id.as_str()) == Some(terms.id.as_str())
            && !matches!(self, Self::None)
        {
            return;
        }
        *self = Self::Offered(terms);
    }

    /// The terms the box shows.
    pub fn terms(&self) -> Option<&TermsOfService> {
        match self {
            Self::None => None,
            Self::Offered(terms) | Self::Accepting(terms) | Self::Failed { terms, .. } => {
                Some(terms)
            }
        }
    }

    pub fn is_accepting(&self) -> bool {
        matches!(self, Self::Accepting(_))
    }

    /// The failure line of the accept.
    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed { error, .. } => Some(error),
            _ => None,
        }
    }

    /// The accept goes out (again, after a failure).
    pub fn begin_accept(&mut self) {
        if let Self::Offered(terms) | Self::Failed { terms, .. } = self {
            *self = Self::Accepting(terms.clone());
        }
    }

    /// The accept failed to go out or came back with `error`.
    pub fn fail_accept(&mut self, error: &str) {
        if let Self::Offered(terms) | Self::Accepting(terms) | Self::Failed { terms, .. } = self {
            *self = Self::Failed {
                terms: terms.clone(),
                error: error.to_string(),
            };
        }
    }

    /// `id` was accepted (here or elsewhere): only those terms go; newer
    /// ones pushed meanwhile stay for the user to answer.
    pub fn accepted(&mut self, id: Option<&str>) {
        if self.terms().map(|t| t.id.as_str()) == id {
            *self = Self::None;
        }
    }

    /// The terms are gone (a demo session answers them directly).
    pub fn dismiss(&mut self) {
        *self = Self::None;
    }

    /// The failure of the accept for `id`; an older terms' answer is not
    /// the current box's.
    pub fn fail_accept_of(&mut self, id: Option<&str>, error: &str) {
        if self.terms().map(|t| t.id.as_str()) == id {
            self.fail_accept(error);
        }
    }
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

/// What `updateUnconfirmedSession` announced, with the sessions behind it
/// once `getActiveSessions` resolved them: one value, replaced whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnconfirmedAlert {
    /// The first unconfirmed login of the last update.
    pub first: Option<UnconfirmedLogin>,
    /// The total count of unconfirmed logins (always above zero).
    pub count: i32,
    /// Sessions behind the alert, resolved from `getActiveSessions`.
    pub entries: Vec<UnconfirmedEntry>,
}

#[derive(Debug, Clone, Default)]
pub struct AccountNotices {
    /// The new-login alert: `None` when every login is confirmed.
    pub unconfirmed: Option<UnconfirmedAlert>,
    /// `confirmSession` / `terminateSession` requests of the review.
    pub review: ReviewStage,
    /// Popups in arrival order; the UI shows the front one.
    pub service: VecDeque<ServiceNotice>,
    /// Terms the user must accept, and the accept round-trip.
    pub terms: TermsState,
}

impl AccountNotices {
    /// The unconfirmed logins announced (0 when none).
    pub fn unconfirmed_count(&self) -> i32 {
        self.unconfirmed.as_ref().map_or(0, |alert| alert.count)
    }

    /// The sessions behind the alert, once resolved.
    pub fn unconfirmed_entries(&self) -> &[UnconfirmedEntry] {
        self.unconfirmed
            .as_ref()
            .map_or(&[], |alert| alert.entries.as_slice())
    }
}

/// Where the two-step recovery/reset flow is, as the dialog needs it. One
/// step at a time: a step that starts or finishes replaces what the last one
/// left, and the dialog going back to its status view clears it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum TwofaFlow {
    #[default]
    Idle,
    /// `requestPasswordRecovery` answered: the emailed code the step awaits
    /// went to this address pattern.
    RecoveryCode(String),
    /// `setLoginEmailAddress` answered: the code went to this pattern.
    LoginEmailCode(String),
    /// A finished step the dialog reports until it moves on.
    Done(TwofaNotice),
}

impl TwofaFlow {
    /// Where the recovery code went, while it is awaited.
    pub fn recovery(&self) -> Option<&str> {
        match self {
            Self::RecoveryCode(to) => Some(to),
            _ => None,
        }
    }

    /// Where the login-email code went, while it is awaited.
    pub fn login_email(&self) -> Option<&str> {
        match self {
            Self::LoginEmailCode(to) => Some(to),
            _ => None,
        }
    }

    /// The finished step's report.
    pub fn notice(&self) -> Option<TwofaNotice> {
        match self {
            Self::Done(notice) => Some(*notice),
            _ => None,
        }
    }

    /// The recovery step is over; another step's wait stays.
    pub fn end_recovery(&mut self) {
        if matches!(self, Self::RecoveryCode(_)) {
            *self = Self::Idle;
        }
    }

    /// The login-email step is over; another step's wait stays.
    pub fn end_login_email(&mut self) {
        if matches!(self, Self::LoginEmailCode(_)) {
            *self = Self::Idle;
        }
    }

    /// A step finished: its report replaces the step before it.
    pub fn finish(&mut self, notice: TwofaNotice) {
        *self = Self::Done(notice);
    }

    /// The dialog is back at its status view or closed.
    pub fn clear(&mut self) {
        *self = Self::Idle;
    }

    /// A step awaits its code, and the dialog drops the report line when a
    /// new form opens.
    pub fn clear_notice(&mut self) {
        if matches!(self, Self::Done(_)) {
            *self = Self::Idle;
        }
    }
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
        if count <= 0 {
            self.settings.notices.unconfirmed = None;
            return;
        }
        // The sessions already resolved stay until the answer replaces them.
        let entries = self
            .settings
            .notices
            .unconfirmed
            .take()
            .map(|alert| alert.entries)
            .unwrap_or_default();
        self.settings.notices.unconfirmed = Some(UnconfirmedAlert {
            first: session,
            count,
            entries,
        });
        // The update carries no session id: refetch the list; its answer
        // fills the entries.
        self.settings.sessions_op.mark_stale();
    }

    /// `getActiveSessions` answered: resolve the sessions behind the
    /// alert from `is_unconfirmed`. Without an alert there is nothing to
    /// resolve: the update that announces one asks again.
    pub(crate) fn resolve_unconfirmed_entries(&mut self, sessions: &[ParsedSession]) {
        let Some(alert) = self.settings.notices.unconfirmed.as_mut() else {
            return;
        };
        alert.entries = sessions
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
        self.settings
            .notices
            .service
            .push_back(ServiceNotice { kind, text });
    }

    /// The front popup is dismissed.
    pub fn dismiss_service_notice(&mut self) {
        self.settings.notices.service.pop_front();
    }

    /// The review starts: one request per session still unreviewed, sent
    /// next; each is told its request by [`Session::login_review_sent`].
    pub(crate) fn begin_login_review(&mut self, confirmed: bool) {
        let done = self.settings.notices.review.done();
        let items = self
            .settings
            .notices
            .unconfirmed_entries()
            .iter()
            .map(|entry| ReviewItem {
                entry: entry.clone(),
                stage: ReviewItemStage::ToSend,
            })
            .collect();
        self.settings.notices.review = ReviewStage::Reviewing {
            confirmed,
            items,
            done,
            error: None,
        };
    }

    /// The request for session `id` went out.
    pub(crate) fn login_review_sent(&mut self, id: i64, request: RequestId) {
        if let ReviewStage::Reviewing { items, .. } = &mut self.settings.notices.review
            && let Some(item) = items
                .iter_mut()
                .find(|item| item.stage == ReviewItemStage::ToSend && item.entry.id == id)
        {
            item.stage = ReviewItemStage::InFlight(request);
        }
    }

    /// The request for session `id` could not be sent.
    pub(crate) fn login_review_unsent(&mut self, id: i64, error: String) {
        let at = match &self.settings.notices.review {
            ReviewStage::Reviewing { items, .. } => items
                .iter()
                .position(|item| item.stage == ReviewItemStage::ToSend && item.entry.id == id),
            _ => None,
        };
        if let Some(at) = at {
            self.settle_login_review(at, Some(error));
        }
    }

    /// The `confirmSession` / `terminateSession` under `request` finished.
    /// Only a request of this review counts.
    pub(crate) fn finish_login_review(
        &mut self,
        request: Option<RequestId>,
        error: Option<String>,
    ) {
        let at = match (&self.settings.notices.review, request) {
            (ReviewStage::Reviewing { items, .. }, Some(id)) => items
                .iter()
                .position(|item| item.stage == ReviewItemStage::InFlight(id)),
            _ => None,
        };
        if let Some(at) = at {
            self.settle_login_review(at, error);
        }
    }

    fn settle_login_review(&mut self, at: usize, error: Option<String>) {
        let notices = &mut self.settings.notices;
        let ReviewStage::Reviewing {
            confirmed,
            items,
            done,
            error: failed,
        } = &mut notices.review
        else {
            return;
        };
        if at >= items.len() {
            return;
        }
        let item = items.remove(at);
        match error {
            Some(error) => {
                failed.get_or_insert(error);
            }
            None => {
                if let Some(alert) = notices.unconfirmed.as_mut()
                    && let Some(at) = alert.entries.iter().position(|e| e.id == item.entry.id)
                {
                    done.push(alert.entries.remove(at));
                } else {
                    done.push(item.entry);
                }
            }
        }
        if !items.is_empty() {
            return;
        }
        let confirmed = *confirmed;
        let done = std::mem::take(done);
        match failed.take() {
            Some(error) => notices.review = ReviewStage::Failed { error, done },
            None => {
                let places = if confirmed {
                    Vec::new()
                } else {
                    done.iter()
                        .map(|entry| unconfirmed_place(&entry.device, &entry.location))
                        .collect()
                };
                notices.review = ReviewStage::Reviewed(if confirmed {
                    LoginReview::Allowed
                } else {
                    LoginReview::Prevented { places }
                });
                notices.unconfirmed = None;
            }
        }
        self.settings.sessions_op.mark_stale();
    }

    pub(crate) fn apply_email_code_info(&mut self, op: PasswordOp, pattern: String) {
        match op {
            PasswordOp::RequestRecoveryCode => {
                self.auth_state.twofa_flow = TwofaFlow::RecoveryCode(pattern);
            }
            PasswordOp::SetLoginEmail => {
                self.auth_state.twofa_flow = TwofaFlow::LoginEmailCode(pattern);
            }
            _ => {}
        }
    }

    pub(crate) fn apply_reset_password_result(&mut self, outcome: ResetPasswordOutcome) {
        self.auth_state.twofa_flow.finish(match outcome {
            ResetPasswordOutcome::Ok => TwofaNotice::PasswordRemoved,
            ResetPasswordOutcome::Pending { reset_date } => {
                TwofaNotice::ResetPending { reset_date }
            }
            ResetPasswordOutcome::Declined { retry_date } => {
                TwofaNotice::ResetDeclined { retry_date }
            }
        });
        // The pending date (or the removed password) lives in the state.
        self.auth_state.password_state.mark_outdated();
    }

    /// A `PasswordStateOp` answered `passwordState`.
    pub(crate) fn apply_password_state_op(&mut self, op: PasswordOp) {
        match op {
            PasswordOp::RecoverPassword => {
                let still_on = self
                    .auth_state
                    .password_state
                    .get()
                    .is_some_and(|s| s.has_password);
                self.auth_state.twofa_flow.finish(if still_on {
                    TwofaNotice::PasswordRecovered
                } else {
                    TwofaNotice::PasswordRemoved
                });
            }
            PasswordOp::CheckEmailCode => {
                self.auth_state
                    .twofa_flow
                    .finish(TwofaNotice::RecoveryEmailConfirmed);
            }
            _ => {}
        }
    }

    /// A `PasswordStateOp` answered `ok`.
    pub(crate) fn apply_password_op_ok(&mut self, op: PasswordOp, request: Option<RequestId>) {
        self.auth_state.settle_password_op(request);
        match op {
            PasswordOp::CancelPasswordReset => {
                self.auth_state
                    .twofa_flow
                    .finish(TwofaNotice::ResetCancelled);
                self.auth_state.password_state.mark_outdated();
            }
            PasswordOp::CheckLoginEmailCode => {
                self.auth_state
                    .twofa_flow
                    .finish(TwofaNotice::LoginEmailChanged);
                self.auth_state.password_state.mark_outdated();
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
                    format!(
                        "We detected new {count} logins to your account from {country}. Is it you?"
                    )
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

    fn terms(id: &str) -> TermsOfService {
        TermsOfService {
            id: id.into(),
            text: "Be nice.".into(),
            min_user_age: 0,
            show_popup: true,
        }
    }

    #[test]
    fn terms_pushed_again_keep_the_accept_in_flight() {
        let mut state = TermsState::Offered(terms("tos-1"));
        state.begin_accept();
        state.offer(terms("tos-1"));
        assert!(state.is_accepting(), "no second accept is offered");
        state.fail_accept("no");
        state.offer(terms("tos-1"));
        assert_eq!(state.error(), Some("no"));
        state.offer(terms("tos-2"));
        assert_eq!(state, TermsState::Offered(terms("tos-2")));
    }

    fn session_with_alert(ids: &[i64]) -> Session {
        let sink: std::sync::Arc<dyn crate::diagnostics::DiagnosticSink> =
            std::sync::Arc::new(crate::diagnostics::MemorySink::new());
        let mut session = Session::new(crate::ids::AccountKey::primary(), sink);
        session.settings.notices.unconfirmed = Some(UnconfirmedAlert {
            first: None,
            count: i32::try_from(ids.len()).unwrap_or(1),
            entries: ids.iter().map(|id| entry(*id, "d", "l")).collect(),
        });
        session
    }

    #[test]
    fn a_failed_send_does_not_leave_the_review_pending() {
        let mut session = session_with_alert(&[1, 2, 3]);
        // Three sends planned: the first went out, the second failed, so it
        // and the third are finished with the error; the answer still out
        // settles the review as failed.
        session.begin_login_review(true);
        session.login_review_sent(1, RequestId(11));
        session.login_review_unsent(2, "failed".into());
        session.login_review_unsent(3, "failed".into());
        assert!(session.settings.notices.review.busy());
        session.finish_login_review(Some(RequestId(11)), None);
        assert!(matches!(
            session.settings.notices.review,
            ReviewStage::Failed { .. }
        ));
    }

    #[test]
    fn an_answer_counts_only_for_the_request_that_asked() {
        let mut session = session_with_alert(&[1, 2]);
        session.begin_login_review(false);
        session.login_review_sent(1, RequestId(11));
        session.login_review_sent(2, RequestId(12));
        // An answer of some other request, or one that names none, changes
        // nothing; a request answered twice counts once.
        session.finish_login_review(Some(RequestId(99)), None);
        session.finish_login_review(None, None);
        session.finish_login_review(Some(RequestId(11)), None);
        session.finish_login_review(Some(RequestId(11)), None);
        assert!(session.settings.notices.review.busy());
        // The lost request settles as an error, so the review ends.
        session.finish_login_review(Some(RequestId(12)), Some("lost".into()));
        assert!(matches!(
            session.settings.notices.review,
            ReviewStage::Failed { .. }
        ));
        assert_eq!(session.settings.notices.unconfirmed_entries().len(), 1);
    }

    #[test]
    fn a_count_of_zero_drops_the_alert_whole() {
        let mut session = session_with_alert(&[1]);
        session.apply_unconfirmed_session(None, 0);
        assert_eq!(session.settings.notices.unconfirmed_count(), 0);
        assert!(session.settings.notices.unconfirmed_entries().is_empty());
        // Sessions that answer with no alert announced are not kept.
        session.resolve_unconfirmed_entries(&[]);
        assert!(session.settings.notices.unconfirmed.is_none());
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
