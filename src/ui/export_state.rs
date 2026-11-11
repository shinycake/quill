//! The export boxes' stage, one value per account screen.
//!
//! "Export chat history" and "Export Telegram data" share a shape: an
//! options box with a draft, then the running export's progress. Whether
//! the progress is on screen or hidden lives with the export itself
//! (`ExportView`, held by the account's session), and whether an export
//! exists is read from the session.

/// What the box shows besides a running export.
pub(super) enum ExportBox<D, P> {
    /// No options draft: the running export's progress shows (when the
    /// export is not hidden). `seen` is the last progress drawn, so the
    /// poll redraws only when it changes.
    Progress { seen: Option<P> },
    /// The options form, with the user's choices so far.
    Options(D),
}

impl<D, P> Default for ExportBox<D, P> {
    fn default() -> Self {
        Self::Progress { seen: None }
    }
}

impl<D, P: PartialEq> ExportBox<D, P> {
    pub(super) fn draft(&self) -> Option<&D> {
        match self {
            Self::Options(draft) => Some(draft),
            Self::Progress { .. } => None,
        }
    }

    pub(super) fn draft_mut(&mut self) -> Option<&mut D> {
        match self {
            Self::Options(draft) => Some(draft),
            Self::Progress { .. } => None,
        }
    }

    pub(super) fn is_options(&self) -> bool {
        matches!(self, Self::Options(_))
    }

    /// Leave the options form (Cancel, close, started) and forget the
    /// progress drawn.
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    /// Record `progress` as drawn; whether it differs from the last one.
    pub(super) fn note_seen(&mut self, progress: P) -> bool {
        match self {
            Self::Progress { seen } => {
                let changed = seen.as_ref() != Some(&progress);
                *seen = Some(progress);
                changed
            }
            Self::Options(_) => false,
        }
    }

    /// Forget the progress drawn (the export is gone).
    pub(super) fn forget_seen(&mut self) {
        if let Self::Progress { seen } = self {
            *seen = None;
        }
    }
}
