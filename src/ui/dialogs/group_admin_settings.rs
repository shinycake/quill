use quill::ids::ChatId;

/// Which page an Edit box row opened.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum GroupSettingsView {
    Reactions,
    Discussion,
    /// "Upgrade to supergroup?": Editing until Upgrade, Sending while
    /// the request is out, Failed with TDLib's reason; it closes on the
    /// answer or on the chat's `pump_chat_upgrades` event.
    ConfirmUpgrade(quill::state::Submit),
    ConfirmUnlink,
    Boosts,
    InviteLinks,
    JoinRequests,
    Administrators,
    RecentActions,
    /// The Filter box over the recent actions, with its choices so far:
    /// they live and go with this page, so they are always this chat's.
    RecentActionsFilter(Box<crate::ui::event_log_filter_box::EventLogFilterBox>),
}

/// The page dialog over the group / channel Edit box (reactions,
/// discussion group, invite links, join requests, administrators, recent
/// actions, boosts, the basic group upgrade). One dialog, one page at a
/// time; the Edit box shows again when it closes.
///
/// The pages the Invite Links section opened (a link's "who joined"
/// details, the revoked list) are session fetches (`InvitePages`,
/// `revoked_invite_links`): open means the fetch exists, and they go with
/// the box when it is replaced for another chat
/// (`QuillApp::set_group_settings_dialog`).
pub struct GroupSettingsDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) view: GroupSettingsView,
}

impl GroupSettingsDialog {
    pub(crate) fn new(chat_id: ChatId, view: GroupSettingsView) -> Self {
        Self { chat_id, view }
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
}
