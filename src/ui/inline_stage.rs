//! The inline-mode dropdown of one chat window: closed, or open with the
//! highlighted row and what the typed `@bot query` is waiting for. One value
//! (docs/contributing/state-machines.md), so closing the panel also drops its
//! pending debounce timer and its wait for the session.

use gpui_kit::Task;

/// What an open panel's query waits for.
#[derive(Default)]
pub(super) enum InlineWait {
    /// Nothing: the query went out or none is due.
    #[default]
    Idle,
    /// The query waits for the session: the bot is being resolved, or an
    /// older query of this chat and bot is in flight. `progress_inline_mode`
    /// sends it when updates land, in whichever window this value is in.
    Session,
    /// A debounce timer is armed for `(username, query)`. Dropping the value
    /// cancels the timer, so a newer keystroke, a close or a chat change
    /// needs no token.
    Armed {
        username: String,
        query: String,
        _timer: Task<()>,
    },
}

#[derive(Default)]
pub(super) enum InlineMode {
    #[default]
    Closed,
    Open {
        /// The highlighted row (Up/Down/Enter).
        selected: usize,
        wait: InlineWait,
    },
}

impl InlineMode {
    pub(super) fn is_open(&self) -> bool {
        matches!(self, Self::Open { .. })
    }

    /// Shows the panel; an open one keeps its row and its wait.
    pub(super) fn open(&mut self) {
        if !self.is_open() {
            *self = Self::Open {
                selected: 0,
                wait: InlineWait::Idle,
            };
        }
    }

    /// Closes the panel and drops what it waited for.
    pub(super) fn close(&mut self) {
        *self = Self::Closed;
    }

    pub(super) fn selected(&self) -> usize {
        match self {
            Self::Open { selected, .. } => *selected,
            Self::Closed => 0,
        }
    }

    pub(super) fn select(&mut self, row: usize) {
        if let Self::Open { selected, .. } = self {
            *selected = row;
        }
    }

    /// A fresh open panel whose query waits for the session.
    pub(super) fn open_waiting(&mut self) {
        *self = Self::Open {
            selected: 0,
            wait: InlineWait::Session,
        };
    }

    /// Waits for the session again (an older query was in flight).
    pub(super) fn wait_for_session(&mut self) {
        if let Self::Open { wait, .. } = self {
            *wait = InlineWait::Session;
        }
    }

    /// Takes the wait for the session: true when the panel was waiting.
    pub(super) fn take_session_wait(&mut self) -> bool {
        match self {
            Self::Open { wait, .. } if matches!(wait, InlineWait::Session) => {
                *wait = InlineWait::Idle;
                true
            }
            _ => false,
        }
    }

    /// Whether the debounce timer of this exact query is armed.
    pub(super) fn is_armed_for(&self, username: &str, query: &str) -> bool {
        matches!(
            self,
            Self::Open { wait: InlineWait::Armed { username: u, query: q, .. }, .. }
                if u == username && q == query
        )
    }

    /// Arms a timer, replacing (and so cancelling) any earlier one. A closed
    /// panel arms nothing; the timer is dropped with `timer`.
    pub(super) fn arm(&mut self, username: &str, query: &str, timer: Task<()>) {
        if let Self::Open { wait, .. } = self {
            *wait = InlineWait::Armed {
                username: username.to_string(),
                query: query.to_string(),
                _timer: timer,
            };
        }
    }

    /// The timer fired: it is no longer armed.
    pub(super) fn timer_fired(&mut self) {
        if let Self::Open { wait, .. } = self
            && matches!(wait, InlineWait::Armed { .. })
        {
            *wait = InlineWait::Idle;
        }
    }
}
