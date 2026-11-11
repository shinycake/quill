//! The while-authorized change-number flow (Slice A8): a code sent to the
//! new number, a resend, a check. Each round trip is a step that carries
//! its request, so only that request's answer or error moves it.

use crate::ids::RequestId;

/// One round trip of the flow.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ChangeStep {
    #[default]
    Idle,
    /// Out under this request; its answer or error ends it.
    InFlight(RequestId),
    /// The last one failed with this honest, classified line (never the
    /// native message). It goes with the next attempt.
    Failed(String),
}

impl ChangeStep {
    pub fn in_flight(&self) -> bool {
        matches!(self, Self::InFlight(_))
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed(error) => Some(error),
            _ => None,
        }
    }

    /// `request` answered or failed: whether it was the one in flight.
    /// The step goes back to `Idle` (a failure then sets its line).
    fn settle(&mut self, request: Option<RequestId>) -> bool {
        match (&*self, request) {
            (Self::InFlight(waiting), Some(id)) if *waiting == id => {
                *self = Self::Idle;
                true
            }
            _ => false,
        }
    }
}

/// The code the server sent: the target number (the code-entry step needs
/// it; the code itself is never stored, the A2 rule) and the seconds
/// before a resend is allowed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeSent {
    pub phone: String,
    pub timeout: i32,
}

/// The flow. `send` is a `sendPhoneNumberCode` or `resendPhoneNumberCode`,
/// `check` a `checkPhoneNumberCode`; a check may run during a send because
/// a resend is number-stable and a user may verify an already-received
/// code while it round-trips.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangeNumber {
    pub code: Option<CodeSent>,
    pub send: ChangeStep,
    pub check: ChangeStep,
}

impl ChangeNumber {
    pub fn phone(&self) -> Option<&str> {
        self.code.as_ref().map(|code| code.phone.as_str())
    }

    pub fn timeout(&self) -> Option<i32> {
        self.code.as_ref().map(|code| code.timeout)
    }

    /// A send, resend or check is out.
    pub fn busy(&self) -> bool {
        self.send.in_flight() || self.check.in_flight()
    }

    /// The failure line of the last attempt.
    pub fn error(&self) -> Option<&str> {
        self.send.error().or_else(|| self.check.error())
    }

    /// A send or resend goes out; the last failure is gone.
    pub fn begin_send(&mut self, request: RequestId) {
        self.clear_errors();
        self.send = ChangeStep::InFlight(request);
    }

    /// A check goes out; the last failure is gone.
    pub fn begin_check(&mut self, request: RequestId) {
        self.clear_errors();
        self.check = ChangeStep::InFlight(request);
    }

    /// The request never left.
    pub fn unsent(&mut self, request: RequestId) {
        self.send.settle(Some(request));
        self.check.settle(Some(request));
    }

    fn clear_errors(&mut self) {
        for step in [&mut self.send, &mut self.check] {
            if matches!(step, ChangeStep::Failed(_)) {
                *step = ChangeStep::Idle;
            }
        }
    }

    /// The send / resend answered with the code's target and timeout.
    /// Whether it was the one in flight (a late answer of a superseded
    /// send is not).
    pub fn code_sent(&mut self, request: RequestId, phone: String, timeout: i32) -> bool {
        if !self.send.settle(Some(request)) {
            return false;
        }
        self.code = Some(CodeSent { phone, timeout });
        true
    }

    /// The send / resend failed. The pending number and timeout stay: a
    /// failed send never reached the server, and a refused resend does not
    /// abort the existing verification.
    pub fn send_failed(&mut self, request: Option<RequestId>, line: String) -> bool {
        let ours = self.send.settle(request);
        if ours {
            self.send = ChangeStep::Failed(line);
        }
        ours
    }

    /// The check failed; the pending number stays.
    pub fn check_failed(&mut self, request: Option<RequestId>, line: String) -> bool {
        let ours = self.check.settle(request);
        if ours {
            self.check = ChangeStep::Failed(line);
        }
        ours
    }

    /// The check was accepted: the number is changed and the flow is over.
    /// The target the code went to, for the own user's phone.
    pub fn finish(&mut self) -> Option<String> {
        let target = self.code.take().map(|code| code.phone);
        *self = Self::default();
        target
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_request_in_flight_moves_a_step() {
        let mut flow = ChangeNumber::default();
        flow.begin_send(RequestId(2));
        // A superseded send's answer and error change nothing.
        assert!(!flow.code_sent(RequestId(1), "+1".into(), 60));
        assert!(!flow.send_failed(Some(RequestId(1)), "late".into()));
        assert!(flow.send.in_flight() && flow.code.is_none());
        assert!(flow.code_sent(RequestId(2), "+1".into(), 60));
        assert_eq!(flow.phone(), Some("+1"));
        assert_eq!(flow.timeout(), Some(60));
        assert!(!flow.busy());
    }

    #[test]
    fn a_failure_goes_with_the_next_attempt_and_a_check_may_overlap_a_send() {
        let mut flow = ChangeNumber::default();
        flow.begin_send(RequestId(1));
        flow.code_sent(RequestId(1), "+1".into(), 30);
        flow.begin_send(RequestId(2));
        flow.begin_check(RequestId(3));
        assert!(flow.send.in_flight() && flow.check.in_flight());
        assert!(flow.check_failed(Some(RequestId(3)), "wrong code".into()));
        assert_eq!(flow.error(), Some("wrong code"));
        assert_eq!(flow.phone(), Some("+1"), "the pending number stays");
        flow.begin_check(RequestId(4));
        assert_eq!(flow.error(), None);
        // The accepted check ends the whole flow, a send still out too.
        assert_eq!(flow.finish().as_deref(), Some("+1"));
        assert_eq!(flow, ChangeNumber::default());
    }

    #[test]
    fn a_send_that_never_left_does_not_stay_in_flight() {
        let mut flow = ChangeNumber::default();
        flow.begin_send(RequestId(7));
        flow.unsent(RequestId(7));
        assert!(!flow.busy());
    }
}
