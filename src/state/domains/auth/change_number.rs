//! The while-authorized change-number flow (Slice A8) as one stage.
//!
//! The code itself is never stored (the A2 rule: secrets ride the request
//! JSON only); the target number and the resend timeout are, because the
//! code-entry step needs them.

/// A verification code the server sent: the target number and the seconds
/// before a resend is allowed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeSent {
    pub phone: String,
    pub timeout: i32,
}

/// Where the change-number flow is. Every stage ends by the request's
/// answer or error; a failed send keeps the code that was valid before it
/// (a refused resend does not abort the existing verification).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ChangeNumber {
    #[default]
    Idle,
    /// `sendPhoneNumberCode` is out; `code` is the one sent before.
    Sending { code: Option<CodeSent> },
    /// The code is sent; the user types it.
    Ready(CodeSent),
    /// `resendPhoneNumberCode` is out.
    Resending(CodeSent),
    /// `checkPhoneNumberCode` is out (a check may start while a resend
    /// round-trips: the user may verify a code already received).
    Checking(CodeSent),
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
            Self::Sending { code } | Self::Failed { code, .. } => code.as_ref(),
            Self::Ready(code) | Self::Resending(code) | Self::Checking(code) => Some(code),
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
        matches!(self, Self::Sending { .. } | Self::Resending(_))
    }

    /// A check round trip is in flight.
    pub fn is_checking(&self) -> bool {
        matches!(self, Self::Checking(_))
    }

    /// The failure line of the last operation.
    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed { error, .. } => Some(error),
            _ => None,
        }
    }

    /// A fresh send goes out. One change-number op at a time.
    pub fn begin_send(&mut self) -> bool {
        if self.is_sending() || self.is_checking() {
            return false;
        }
        *self = Self::Sending {
            code: self.sent().cloned(),
        };
        true
    }

    /// A resend goes out: it needs a code already sent.
    pub fn begin_resend(&mut self) -> bool {
        if self.is_sending() || self.is_checking() {
            return false;
        }
        let Some(code) = self.sent().cloned() else {
            return false;
        };
        *self = Self::Resending(code);
        true
    }

    /// A check goes out: it needs a code already sent, and may run during
    /// a resend.
    pub fn begin_check(&mut self) -> bool {
        if self.is_checking() {
            return false;
        }
        let Some(code) = self.sent().cloned() else {
            return false;
        };
        *self = Self::Checking(code);
        true
    }

    /// The request never left (a closing connection): the stage before it.
    pub fn abort(&mut self) {
        *self = match self.sent().cloned() {
            Some(code) => Self::Ready(code),
            None => Self::Idle,
        };
    }

    /// The send, resend or check came back with `error`.
    /// A resend that fails while a check runs leaves the check alone.
    pub fn fail(&mut self, error: String, check: bool) {
        if !check && self.is_checking() {
            return;
        }
        *self = Self::Failed {
            error,
            code: self.sent().cloned(),
        };
    }

    /// The send/resend answer: the code is out. A check in flight stays.
    pub fn code_sent(&mut self, code: CodeSent) {
        *self = match self {
            Self::Checking(_) => Self::Checking(code),
            _ => Self::Ready(code),
        };
    }

    /// The number changed (or the flow is over): back to the start.
    pub fn finish(&mut self) {
        *self = Self::Idle;
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

    #[test]
    fn a_refused_resend_keeps_the_code_and_the_next_attempt_clears_the_line() {
        let mut flow = ChangeNumber::Idle;
        assert!(flow.begin_send());
        flow.code_sent(code());
        assert!(flow.begin_resend());
        assert!(!flow.begin_resend(), "one send at a time");
        flow.fail("no".into(), false);
        assert_eq!(flow.error(), Some("no"));
        assert_eq!(flow.phone(), Some("+15550199"));
        assert!(flow.begin_resend());
        assert_eq!(flow.error(), None);
    }

    #[test]
    fn a_check_runs_during_a_resend_and_survives_its_failure() {
        let mut flow = ChangeNumber::Ready(code());
        assert!(flow.begin_resend());
        assert!(flow.begin_check());
        flow.fail("resend refused".into(), false);
        assert!(flow.is_checking(), "the check is what the user waits for");
        flow.code_sent(CodeSent {
            phone: "+15550199".into(),
            timeout: 30,
        });
        assert!(flow.is_checking());
        assert_eq!(flow.timeout(), Some(30));
        flow.fail("bad code".into(), true);
        assert_eq!(flow.error(), Some("bad code"));
        assert!(flow.begin_check(), "the number stays for a retry");
        flow.finish();
        assert_eq!(flow, ChangeNumber::Idle);
    }

    #[test]
    fn a_send_that_never_left_goes_back() {
        let mut flow = ChangeNumber::Idle;
        assert!(flow.begin_send());
        flow.abort();
        assert_eq!(flow, ChangeNumber::Idle);
        assert!(!flow.begin_resend() && !flow.begin_check(), "no code yet");
    }
}
