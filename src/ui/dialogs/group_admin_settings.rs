use quill::ids::ChatId;

/// B7: which page of the group settings dialog is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GroupSettingsView {
    Main,
    Reactions,
    Discussion,
    ConfirmUpgrade,
    ConfirmUnlink,
    Boosts,
}

/// B7: the group / channel settings dialog (topics, history, hidden
/// members, discussion group, reactions, upgrade, boosts). One dialog,
/// several pages; who may write, content protection and the links live in
/// the Type box.
pub struct GroupSettingsDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) view: GroupSettingsView,
}

/// B7: one change the dialog can request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GroupSettingsAction {
    Topics(bool),
    HistoryVisible(bool),
    HiddenMembers(bool),
    ReactionsAll,
    ReactionsNone,
    ReactionsSome,
    ToggleEmoji(String),
    PaidReaction(bool),
    LinkGroup(ChatId),
    Unlink,
    Upgrade,
}
