//! The one account write (`setAccountTtl` or `deleteAccount`) that may be
//! out at a time, and why the last one was refused.
use crate::ids::RequestId;

/// Where the account write stands. It is the only record of a write in
/// flight (the TTL and delete surfaces read their spinner and their error
/// from it), holds the request it waits for, and ends with that request's
/// answer, its error, or the request being lost (a lost request settles
/// through the error path).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum AccountWrite {
    #[default]
    Idle,
    /// `setAccountTtl` is out as `request`.
    SetTtl { request: RequestId },
    /// `deleteAccount` is out as `request`.
    Delete { request: RequestId },
    /// The last write was refused (or never left): the line to show. The
    /// next write, or the box opening again, clears it.
    Refused(String),
}

impl AccountWrite {
    /// A write is out: another one waits for it. A `getAccountTtl` in
    /// flight does not count.
    pub fn is_out(&self) -> bool {
        matches!(self, Self::SetTtl { .. } | Self::Delete { .. })
    }

    /// The request of the write in flight.
    pub fn request(&self) -> Option<RequestId> {
        match self {
            Self::SetTtl { request } | Self::Delete { request } => Some(*request),
            Self::Idle | Self::Refused(_) => None,
        }
    }

    /// The reason the last write was refused.
    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Refused(line) => Some(line),
            _ => None,
        }
    }

    /// `request` came back with an answer: its write is over. Another
    /// request's late answer changes nothing.
    pub fn settle(&mut self, request: Option<RequestId>) {
        if request.is_some() && self.request() == request {
            *self = Self::Idle;
        }
    }

    /// `request` came back with an error (or never left): its write ends
    /// in the line. Another request's error changes nothing.
    pub fn refused(&mut self, request: Option<RequestId>, line: impl Into<String>) {
        if request.is_some() && self.request() == request {
            *self = Self::Refused(line.into());
        }
    }

    /// The line was seen (the box opens again).
    pub fn clear_error(&mut self) {
        if matches!(self, Self::Refused(_)) {
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
        write.refused(Some(RequestId(2)), "no");
        assert!(write.is_out());
        write.refused(Some(RequestId(3)), "no");
        assert!(!write.is_out());
        assert_eq!(write.error(), Some("no"));
        write.clear_error();
        assert_eq!(write, AccountWrite::Idle);
    }
}
