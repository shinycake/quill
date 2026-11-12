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
    /// The date lookup of each window (`None` is the main window's): its
    /// ask, then the resolved jump or the note the calendar box draws.
    pub(crate) jump_lookups: crate::state::JumpLookups,
}

/// Where one window's date lookup stands. `Asking` ends with the answer, a
/// refused send, a newer pick of the window, the window's chat leaving
/// (`Session::clear_date_jump_ui`) or the window closing; `Resolved` when
/// the driver starts the jump ([`crate::state::JumpLookups::take_resolved`])
/// or with its chat; `NotFound` when the calendar box opens again, a day
/// is picked, or the chat goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JumpLookup {
    /// `getChatMessageByDate` is out as `request`, asked in `chat`.
    Asking {
        request: RequestId,
        chat: ChatId,
    },
    Resolved(ViewJump),
    /// The picked day of the shared-media calendar has no media near it:
    /// the box draws "No media" until it is opened again.
    NoMedia,
}

impl SearchDomainState {
    pub(crate) fn new() -> Self {
        Self {
            search: SearchState::default(),
            chat_search: ChatSearchState::default(),
            history_calendar: None,
            calendar_generation: 0,
            jump_lookups: Default::default(),
        }
    }
}
