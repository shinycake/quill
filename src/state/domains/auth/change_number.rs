//! The while-authorized change-number flow (Slice A8) as one stage.
//!
//! The code itself is never stored (the A2 rule: secrets ride the request
//! JSON only); the target number and the resend timeout are, because the
//! code-entry step needs them.

use crate::state::RequestId;

/// A verification code the server sent: the target number and the seconds
/// before a resend is allowed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeSent {
    pub phone: String,
    pub timeout: i32,
}

/// Where the change-number flow is. Every in-flight stage carries the
/// request it waits for and takes only that request's answer or error; a
/// failed send keeps the code that was valid before it (a refused resend
/// does not abort the existing verification).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ChangeNumber {
    #[default]
    Idle,
    /// `sendPhoneNumberCode` is out; `code` is the one sent before.
    Sending {
        request: RequestId,
        code: Option<CodeSent>,
    },
    /// The code is sent; the user types it.
    Ready(CodeSent),
    /// `resendPhoneNumberCode` is out.
    Resending { request: RequestId, code: CodeSent },
    /// `checkPhoneNumberCode` is out. A check may start while a resend
    /// round-trips (the user may verify a code already received): that
    /// resend is `resend`, and its answer or failure never ends the check.
    Checking {
        request: RequestId,
        code: CodeSent,
        resend: Option<RequestId>,
    },
    /// The last operation failed with this honest, classified line (never
    /// the native message); the next attempt clears it.
    Failed {
        error: String,
        code: Option<CodeSent>,
    },
}

impl ChangeNumber {
    fn sent(&self) -> Option<&CodeSent> {
        match self {
            Self::Idle => None,
            Self::Sending { code, .. } | Self::Failed { code, .. } => code.as_ref(),
            Self::Ready(code) | Self::Resending { code, .. } | Self::Checking { code, .. } => {
                Some(code)
            }
        }
    }

    /// The target number the code was sent to.
    pub fn phone(&self) -> Option<&str> {
        self.sent().map(|code| code.phone.as_str())
    }

    /// The seconds before a resend is allowed.
    pub fn timeout(&self) -> Option<i32> {
        self.sent().map(|code| code.timeout)
    }

    /// A send or resend round trip is in flight.
    pub fn is_sending(&self) -> bool {
        matches!(
            self,
            Self::Sending { .. }
                | Self::Resending { .. }
                | Self::Checking {
                    resend: Some(_),
                    ..
                }
        )
    }

    /// A check round trip is in flight.
    pub fn is_checking(&self) -> bool {
        matches!(self, Self::Checking { .. })
    }

    /// The failure line of the last operation.
    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed { error, .. } => Some(error),
            _ => None,
        }
    }

    /// A fresh send may go out. One change-number op at a time.
    pub fn can_send(&self) -> bool {
        !self.is_sending() && !self.is_checking()
    }

    /// A resend may go out: it needs a code already sent.
    pub fn can_resend(&self) -> bool {
        self.can_send() && self.sent().is_some()
    }

    /// A check may go out: it needs a code already sent, and may run during
    /// a resend.
    pub fn can_check(&self) -> bool {
        !self.is_checking() && self.sent().is_some()
    }

    /// A fresh send went out as `request`.
    pub fn begin_send(&mut self, request: RequestId) {
        if self.can_send() {
            *self = Self::Sending {
                request,
                code: self.sent().cloned(),
            };
        }
    }

    /// A resend went out as `request`.
    pub fn begin_resend(&mut self, request: RequestId) {
        if self.can_resend()
            && let Some(code) = self.sent().cloned()
        {
            *self = Self::Resending { request, code };
        }
    }

    /// A check went out as `request`; a resend in flight stays under it.
    pub fn begin_check(&mut self, request: RequestId) {
        if !self.can_check() {
            return;
        }
        let resend = match self {
            Self::Resending { request: asked, .. } => Some(*asked),
            _ => None,
        };
        if let Some(code) = self.sent().cloned() {
            *self = Self::Checking {
                request,
                code,
                resend,
            };
        }
    }

    /// `request` never left (a closing connection): the stage before it.
    pub fn abort(&mut self, request: RequestId) {
        self.settle(request, None);
    }

    /// The send, resend or check `request` came back with `error`. An
    /// answer of any other request is not this stage's.
    pub fn fail(&mut self, request: RequestId, error: String) {
        self.settle(request, Some(error));
    }

    /// The send/resend answer for `request`: the code is out. A check in
    /// flight stays.
    pub fn code_sent(&mut self, request: RequestId, new: CodeSent) {
        match self {
            Self::Sending { request: asked, .. } | Self::Resending { request: asked, .. }
                if *asked == request =>
            {
                *self = Self::Ready(new);
            }
            Self::Checking {
                code,
                resend: resend @ Some(_),
                ..
            } if *resend == Some(request) => {
                *code = new;
                *resend = None;
            }
            _ => {}
        }
    }

    /// The number changed (or the flow is over): back to the start. Returns
    /// the send and resend still out, which the caller drops from the
    /// request table so a late answer finds nothing.
    pub fn finish(&mut self) -> Vec<RequestId> {
        let out = match self {
            Self::Sending { request, .. } | Self::Resending { request, .. } => vec![*request],
            Self::Checking {
                resend: Some(resend),
                ..
            } => vec![*resend],
            _ => Vec::new(),
        };
        *self = Self::Idle;
        out
    }

    /// The check `request` succeeded: the number it confirmed and the send
    /// or resend still out (see [`ChangeNumber::finish`]), the flow being
    /// over. `None` for an answer of any other request, which changes
    /// nothing.
    pub fn checked(&mut self, request: RequestId) -> Option<(String, Vec<RequestId>)> {
        let phone = match self {
            Self::Checking {
                request: asked,
                code,
                ..
            } if *asked == request => code.phone.clone(),
            _ => return None,
        };
        Some((phone, self.finish()))
    }

    /// `request` ended: `error` is why it failed, `None` when it never left.
    fn settle(&mut self, request: RequestId, error: Option<String>) {
        let code = self.sent().cloned();
        let resend = match self {
            Self::Checking { resend, .. } => *resend,
            _ => None,
        };
        match self {
            Self::Sending { request: asked, .. } | Self::Resending { request: asked, .. }
                if *asked == request => {}
            Self::Checking {
                resend: resend @ Some(_),
                ..
            } if *resend == Some(request) => {
                // The resend under a check ends; the check goes on.
                *resend = None;
                return;
            }
            Self::Checking { request: asked, .. } if *asked == request => {}
            _ => return,
        }
        *self = match error {
            Some(error) => Self::Failed { error, code },
            None => match (code, resend) {
                (Some(code), Some(request)) => Self::Resending { request, code },
                (Some(code), None) => Self::Ready(code),
                (None, _) => Self::Idle,
            },
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code() -> CodeSent {
        CodeSent {
            phone: "+15550199".into(),
            timeout: 60,
        }
    }

    const A: RequestId = RequestId(1);
    const B: RequestId = RequestId(2);
    const C: RequestId = RequestId(3);

    #[test]
    fn a_refused_resend_keeps_the_code_and_the_next_attempt_clears_the_line() {
        let mut flow = ChangeNumber::Idle;
        assert!(flow.can_send());
        flow.begin_send(A);
        flow.code_sent(A, code());
        assert!(flow.can_resend());
        flow.begin_resend(B);
        assert!(!flow.can_resend(), "one send at a time");
        flow.fail(B, "no".into());
        assert_eq!(flow.error(), Some("no"));
        assert_eq!(flow.phone(), Some("+15550199"));
        assert!(flow.can_resend());
        flow.begin_resend(C);
        assert_eq!(flow.error(), None);
    }

    #[test]
    fn a_check_runs_during_a_resend_and_survives_its_failure() {
        let mut flow = ChangeNumber::Ready(code());
        flow.begin_resend(A);
        assert!(flow.can_check());
        flow.begin_check(B);
        flow.fail(A, "resend refused".into());
        assert!(flow.is_checking(), "the check is what the user waits for");
        assert!(!flow.is_sending(), "the refused resend is over");
        flow.begin_resend(C);
        assert!(!flow.can_resend(), "no resend while a check is out");
        flow.fail(B, "bad code".into());
        assert_eq!(flow.error(), Some("bad code"));
        assert!(flow.can_check(), "the number stays for a retry");
        flow.finish();
        assert_eq!(flow, ChangeNumber::Idle);
    }

    #[test]
    fn a_resend_answer_during_a_check_updates_the_timeout_only() {
        let mut flow = ChangeNumber::Ready(code());
        flow.begin_resend(A);
        flow.begin_check(B);
        flow.code_sent(
            A,
            CodeSent {
                phone: "+15550199".into(),
                timeout: 30,
            },
        );
        assert!(flow.is_checking());
        assert_eq!(flow.timeout(), Some(30));
    }

    #[test]
    fn an_answer_of_another_request_is_not_this_stages() {
        let mut flow = ChangeNumber::Idle;
        flow.begin_send(B);
        flow.fail(A, "old".into());
        flow.code_sent(A, code());
        assert!(flow.is_sending(), "the older request cannot end the send");
        flow.code_sent(B, code());
        assert_eq!(flow, ChangeNumber::Ready(code()));
        flow.begin_check(C);
        flow.fail(A, "old".into());
        flow.abort(B);
        assert!(flow.is_checking());
    }

    #[test]
    fn a_send_that_never_left_goes_back() {
        let mut flow = ChangeNumber::Idle;
        flow.begin_send(A);
        flow.abort(A);
        assert_eq!(flow, ChangeNumber::Idle);
        assert!(!flow.can_resend() && !flow.can_check(), "no code yet");
    }

    #[test]
    fn only_the_check_the_flow_waits_for_finishes_it() {
        let mut flow = ChangeNumber::Idle;
        flow.begin_send(A);
        flow.code_sent(A, code());
        flow.begin_check(B);
        assert_eq!(flow.checked(C), None, "another request's ok");
        assert!(flow.is_checking());
        assert_eq!(flow.checked(B), Some(("+15550199".to_string(), Vec::new())));
        assert_eq!(flow, ChangeNumber::Idle);
        assert_eq!(flow.checked(B), None, "a second ok of the same check");
    }
}
