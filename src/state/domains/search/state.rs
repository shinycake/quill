//! Global search, search in chat and date jumps: the `search` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;

pub struct SearchDomainState {
    pub search: SearchState,
    pub chat_search: ChatSearchState,
    /// The calendar box ("Jump to date"), when open.
    pub history_calendar: Option<HistoryCalendar>,
    /// A resolved date jump the driver has not started yet.
    pub(crate) date_jump: Option<(MessageId, DateJumpMode)>,
    /// A started date jump waiting for its window to load.
    pub(crate) date_jump_pending: Option<(MessageId, DateJumpMode)>,
    /// One-shot note for a date jump that found nothing.
    pub date_jump_note: Option<String>,
}

impl SearchDomainState {
    pub(crate) fn new() -> Self {
        Self {
            search: SearchState::default(),
            chat_search: ChatSearchState::default(),
            history_calendar: None,
            date_jump: None,
            date_jump_pending: None,
            date_jump_note: None,
        }
    }
}
