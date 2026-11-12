//! The one account write (`setAccountTtl` or `deleteAccount`) that may be
//! out at a time.
use crate::ids::RequestId;

/// Where the account write stands. It holds the request it waits for and
/// ends with that request's answer, its error, or the request being lost
/// (a lost request settles through the error path).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AccountWrite {
    #[default]
    Idle,
    /// `setAccountTtl` is out as `request`.
    SetTtl { request: RequestId },
    /// `deleteAccount` is out as `request`.
    Delete { request: RequestId },
}

impl AccountWrite {
    /// A write is out: another one waits for it. A `getAccountTtl` in
    /// flight does not count.
    pub fn is_out(self) -> bool {
        self != Self::Idle
    }

    /// `request` came back (answer or error): its write is over. Another
    /// request's late answer changes nothing.
    pub fn settle(&mut self, request: Option<RequestId>) {
        let waiting = match *self {
            Self::Idle => return,
            Self::SetTtl { request } | Self::Delete { request } => request,
        };
        if request == Some(waiting) {
            *self = Self::Idle;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_writes_own_request_ends_it() {
        let mut write = AccountWrite::SetTtl {
            request: RequestId(3),
        };
        assert!(write.is_out());
        write.settle(Some(RequestId(2)));
        write.settle(None);
        assert!(write.is_out());
        write.settle(Some(RequestId(3)));
        assert!(!write.is_out());
    }
}
