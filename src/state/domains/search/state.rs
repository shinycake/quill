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
    /// What the last date lookup came to: a resolved jump the driver has
    /// not started yet (for the window and chat that asked), or the
    /// "nothing found" note the calendar box draws ([`DateJumpAnswer`]).
    pub(crate) date_jump: DateJumpAnswer,
}

/// The outcome of a date lookup. `Resolved` leaves when the driver starts
/// the jump ([`SearchDomainState::take_date_jump`]), or with its chat
/// (`Session::clear_date_jump_ui`); `NotFound` when the calendar box opens
/// again, a day is picked, or the chat goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DateJumpAnswer {
    #[default]
    Idle,
    Resolved(ViewJump),
    /// The jump found nothing to land on (the note's text is the line
    /// drawn).
    NotFound(DateJumpNote),
}

/// Why a date jump found nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateJumpNote {
    /// `getChatMessageByDate` failed for another reason than "too old".
    Failed,
    /// The picked day of the shared-media calendar has no media near it.
    NoMedia,
}

impl SearchDomainState {
    /// The resolved jump, for the driver to start. A note stays.
    pub(crate) fn take_date_jump(&mut self) -> Option<ViewJump> {
        match self.date_jump {
            DateJumpAnswer::Resolved(jump) => {
                self.date_jump = DateJumpAnswer::Idle;
                Some(jump)
            }
            _ => None,
        }
    }

    /// The line the calendar box draws for a jump that found nothing.
    pub fn date_jump_note(&self) -> Option<String> {
        match self.date_jump {
            DateJumpAnswer::NotFound(DateJumpNote::Failed) => {
                Some("Couldn't jump to that date.".into())
            }
            DateJumpAnswer::NotFound(DateJumpNote::NoMedia) => {
                Some(crate::lang::tr(crate::lang::keys::Q_MISC_NO_MEDIA_TO_JUMP_TO).to_string())
            }
            _ => None,
        }
    }

    /// The "nothing found" note goes; a resolved jump stays.
    pub(crate) fn clear_date_jump_note(&mut self) {
        if matches!(self.date_jump, DateJumpAnswer::NotFound(_)) {
            self.date_jump = DateJumpAnswer::Idle;
        }
    }
}

impl SearchDomainState {
    pub(crate) fn new() -> Self {
        Self {
            search: SearchState::default(),
            chat_search: ChatSearchState::default(),
            history_calendar: None,
            calendar_generation: 0,
            date_jump: DateJumpAnswer::Idle,
        }
    }
}
