//! The export boxes' stage, one value per account screen.
//!
//! "Export chat history" and "Export Telegram data" share a shape: an
//! options box with a draft, then the running export's progress. The box
//! is one value: closed (a running export goes on, its result shows as a
//! note), the options form, or the progress. Whether an export exists is
//! read from the session. The value belongs to the account on screen, so
//! switching accounts closes the box; the export keeps running.

/// What the box shows.
pub(super) enum ExportBox<D, P> {
    /// Not on screen ("Hide", Cancel, close). A running export is not
    /// shown; when it ends, the poll says how it went.
    Closed,
    /// The running export's progress is on screen. `seen` is the last
    /// progress drawn, so the poll redraws only when it changes.
    Progress { seen: Option<P> },
    /// The options form, with the user's choices so far.
    Options(D),
}

impl<D, P> Default for ExportBox<D, P> {
    fn default() -> Self {
        Self::Closed
    }
}

impl<D, P: PartialEq> ExportBox<D, P> {
    pub(super) fn draft(&self) -> Option<&D> {
        match self {
            Self::Options(draft) => Some(draft),
            Self::Closed | Self::Progress { .. } => None,
        }
    }

    pub(super) fn draft_mut(&mut self) -> Option<&mut D> {
        match self {
            Self::Options(draft) => Some(draft),
            Self::Closed | Self::Progress { .. } => None,
        }
    }

    pub(super) fn is_options(&self) -> bool {
        matches!(self, Self::Options(_))
    }

    /// Whether the running export's progress is on screen.
    pub(super) fn is_progress(&self) -> bool {
        matches!(self, Self::Progress { .. })
    }

    /// Leave the box (Cancel, close, Hide, Done, the export is gone).
    pub(super) fn close(&mut self) {
        *self = Self::Closed;
    }

    /// The export is gone: close the progress, but leave an options form.
    pub(super) fn end_progress(&mut self) {
        if self.is_progress() {
            *self = Self::Closed;
        }
    }

    /// Show the running export's progress (an export started, or the box
    /// opened over a running one).
    pub(super) fn show_progress(&mut self) {
        *self = Self::Progress { seen: None };
    }

    /// Record `progress` as drawn; whether it differs from the last one.
    pub(super) fn note_seen(&mut self, progress: P) -> bool {
        match self {
            Self::Progress { seen } => {
                let changed = seen.as_ref() != Some(&progress);
                *seen = Some(progress);
                changed
            }
            Self::Closed | Self::Options(_) => false,
        }
    }
}
