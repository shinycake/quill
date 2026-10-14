use quill::ids::ChatId;

/// B7: which page of the group settings dialog is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GroupSettingsView {
    Main,
    Reactions,
    Discussion,
    ConfirmUpgrade,
    ConfirmUnlink,
    Usernames,
    Boosts,
}

/// B7: the group / channel settings dialog (topics, history, join to
/// send, hidden members, content protection, discussion group, reactions,
/// upgrade). One dialog, several pages.
pub struct GroupSettingsDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) view: GroupSettingsView,
}

/// B7: one change the dialog can request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GroupSettingsAction {
    Topics(bool),
    HistoryVisible(bool),
    JoinToSend(bool),
    HiddenMembers(bool),
    ProtectedContent(bool),
    ReactionsAll,
    ReactionsNone,
    ReactionsSome,
    ToggleEmoji(String),
    PaidReaction(bool),
    LinkGroup(ChatId),
    Unlink,
    Upgrade,
}
