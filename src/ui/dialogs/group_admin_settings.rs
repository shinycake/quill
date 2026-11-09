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
///
/// The Invite Links section's own state is part of this value, and the
/// pages it opened in the session (`InvitePages`) go with it: the box is
/// replaced whole for another chat (`QuillApp::set_group_settings_dialog`).
pub struct GroupSettingsDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) view: GroupSettingsView,
    /// The invite link whose "who joined" details are expanded.
    pub(crate) details_link: Option<String>,
    /// The revoked-links list is expanded (and was fetched).
    pub(crate) revoked_open: bool,
}

impl GroupSettingsDialog {
    pub(crate) fn new(chat_id: ChatId, view: GroupSettingsView) -> Self {
        Self {
            chat_id,
            view,
            details_link: None,
            revoked_open: false,
        }
    }
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
