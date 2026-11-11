//! A launch waiting for an answer, with the window that asked for it.
use super::PendingOpen;
use quill::ids::{ChatId, RequestId};

/// A launch waiting for an answer, with the window that asked for it: the
/// answer's notes and boxes show there, whatever flow started since (the
/// poll keeps no one window for them all).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Awaited {
    /// The send this waits on: only its answer moves the launch on.
    pub(super) request: RequestId,
    pub(super) then: PendingOpen,
    pub(super) window: Option<ChatId>,
}
