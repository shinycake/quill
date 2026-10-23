use quill::ids::ChatId;

/// Which page an Edit box row opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GroupSettingsView {
    Reactions,
    Discussion,
    ConfirmUpgrade,
    ConfirmUnlink,
    Boosts,
    InviteLinks,
    JoinRequests,
    Administrators,
    RecentActions,
    /// The Filter box over the recent actions.
    RecentActionsFilter,
}

/// The page dialog over the group / channel Edit box (reactions,
/// discussion group, invite links, join requests, administrators, recent
/// actions, boosts, the basic group upgrade). One dialog, one page at a
/// time; the Edit box shows again when it closes.
pub struct GroupSettingsDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) view: GroupSettingsView,
}

/// One change a page or an Edit box row can request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GroupSettingsAction {
    Topics(bool),
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
