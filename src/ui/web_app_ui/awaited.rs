//! A launch waiting for an answer, with the window that asked for it.
use super::PendingOpen;
use quill::ids::ChatId;

/// A launch waiting for an answer, with the window that asked for it: the
/// answer's notes and boxes show there, whatever flow started since (the
/// poll keeps no one window for them all).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Awaited {
    pub(super) then: PendingOpen,
    pub(super) window: Option<ChatId>,
}

impl Awaited {
    pub(super) fn bot_id(&self) -> i64 {
        self.then.bot_id()
    }
}
