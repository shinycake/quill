//! Global search, search in chat and date jumps: the `search` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;

pub struct SearchDomainState {
    pub search: SearchState,
    pub chat_search: ChatSearchState,
    /// The calendar box ("Jump to date"), when open.
    pub history_calendar: Option<HistoryCalendar>,
    /// The generation of the last calendar opened. It never goes back when
    /// the box closes: a late page of an earlier calendar must not match a
    /// later one's generation.
    pub(crate) calendar_generation: u64,
    /// A resolved date jump the driver has not started yet, for the
    /// window and chat that asked.
    pub(crate) date_jump: Option<ViewJump>,
    /// A started date jump waiting for its window to load: the chat, the
    /// message loaded around and where to land. Part of a window's view
    /// (`SessionView::date_jump_pending`).
    pub(crate) date_jump_pending: Option<(ChatId, MessageId, DateJumpMode)>,
    /// One-shot note for a date jump that found nothing.
    pub date_jump_note: Option<String>,
}

impl SearchDomainState {
    pub(crate) fn new() -> Self {
        Self {
            search: SearchState::default(),
            chat_search: ChatSearchState::default(),
            history_calendar: None,
            calendar_generation: 0,
            date_jump: None,
            date_jump_pending: None,
            date_jump_note: None,
        }
    }
}
