use quill::ids::{ChatId, MessageId};

/// Which poll view the dialog shows, with the option whose voters are
/// listed (`None` until one is picked).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PollView {
    /// "Voters": the selected option's voters.
    Voters { option: Option<usize> },
    /// "Poll Stats" (`getPollVoteStatistics`): the vote graph above the
    /// lists. Fixed when the dialog opens and never switched.
    Stats { option: Option<usize> },
}

impl PollView {
    pub(crate) fn option(self) -> Option<usize> {
        match self {
            Self::Voters { option } | Self::Stats { option } => option,
        }
    }

    pub(crate) fn shows_stats(self) -> bool {
        matches!(self, Self::Stats { .. })
    }

    fn with_option(self, option: usize) -> Self {
        match self {
            Self::Voters { .. } => Self::Voters {
                option: Some(option),
            },
            Self::Stats { .. } => Self::Stats {
                option: Some(option),
            },
        }
    }
}

/// B4: poll voter-list viewer (`getPollVoters`, schema 1.8.67 line
/// 12941). One dialog per poll message; the selected option's voters
/// page in below it with a "Load more" button (50 per page, the schema
/// max).
pub struct PollVotersDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) message_id: MessageId,
    pub(crate) view: PollView,
}

impl PollVotersDialog {
    pub(crate) fn select(&mut self, option: usize) {
        self.view = self.view.with_option(option);
    }
}

#[cfg(test)]
mod tests {
    use super::{PollView, PollVotersDialog};
    use quill::ids::{ChatId, MessageId};

    #[test]
    fn picking_an_option_keeps_the_view_it_opened_in() {
        let mut dialog = PollVotersDialog {
            chat_id: ChatId(1),
            message_id: MessageId(2),
            view: PollView::Stats { option: None },
        };
        dialog.select(3);
        assert_eq!(dialog.view, PollView::Stats { option: Some(3) });
        dialog.view = PollView::Voters { option: None };
        dialog.select(1);
        assert_eq!(dialog.view, PollView::Voters { option: Some(1) });
        assert!(!dialog.view.shows_stats());
    }
}
