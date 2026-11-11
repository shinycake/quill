use quill::ids::{ChatId, MessageId};
/// B4: poll voter-list viewer (`getPollVoters`, schema 1.8.67 line
/// 12941). One dialog per poll message; the selected option's voters
/// page in below it with a "Load more" button (50 per page, the schema
/// max).
pub struct PollVotersDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) message_id: MessageId,
    pub(crate) selected_option: Option<usize>,
    /// What the dialog opened as.
    pub(crate) view: PollView,
}

/// B15: the voter lists alone, or opened from "Poll Stats" with the vote
/// graph above them (`getPollVoteStatistics`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PollView {
    Voters,
    Stats,
}
