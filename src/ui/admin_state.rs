//! Group and channel management boxes: invite links, admins, members, permissions.
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::ids::ChatId;

pub(crate) struct AdminUi {
    /// The New Link / Edit Link box (`invite_link_box/`).
    pub(super) invite_link_box: Option<super::invite_link_box::InviteLinkBox>,
    /// B8: the invite link whose "who joined" details are expanded.
    pub(super) invite_link_details: Option<(ChatId, String)>,
    /// B8: whether the revoked-links list is expanded.
    pub(super) revoked_links_open: bool,
    /// Phase D3b: admin-management dialog state (promote picker /
    /// rights editor / demote confirm).
    pub(super) admin_dialog: Option<AdminDialog>,
    /// New Group / New Channel (`ui::new_chat`).
    pub(super) create_chat_dialog: Option<super::new_chat::NewChatDialog>,
    /// Slice G10: communities dialog state (create dialog + hub flag).
    pub(super) community: CommunityUi,
    /// Slice G1: member-management dialog (tabs + add section).
    pub(super) member_dialog: Option<MemberDialog>,
    pub(super) permissions_dialog: Option<PermissionsDialog>,
    /// The "Add exception" / "Remove user" picker over the member dialog.
    pub(super) add_special: Option<AddSpecialDialog>,
    /// Slice G1: public username editor.
    pub(super) username_dialog: Option<UsernameDialog>,
    /// Slice G1: restrict/ban dialog.
    pub(super) restrict_dialog: Option<RestrictDialog>,
    pub(super) ownership_dialog: Option<OwnershipDialog>,
    /// Slice G1: delete / leave / broadcast-upgrade / ban confirmations.
    pub(super) group_confirm_dialog: Option<GroupConfirmDialog>,
    /// Slice G2: forum-topic management dialog.
    pub(super) forum_manage_dialog: Option<ForumManageDialog>,
    /// Slice G2: event-log search input for the info panel's
    /// "Recent actions" section (created lazily when the panel opens).
    pub(super) event_log_search: Option<Entity<TextareaState>>,
    /// The recent-actions list (oldest event first, newest at the bottom):
    /// earlier events load when it is scrolled to its top.
    pub(super) event_log_list: ListState,
    /// The event ids the list was last built for (`event_log_list::plan_sync`).
    pub(super) event_log_ids: std::cell::RefCell<Vec<i64>>,
    /// Counts search edits so only the last one searches.
    pub(super) event_log_keystroke: u64,
    /// The list reports its scrolling to the app (set up once).
    pub(super) event_log_hooked: bool,
    /// The list length an automatic "load earlier" was last asked for.
    pub(super) event_log_autoload: std::cell::Cell<usize>,
    /// The open filter box (Save applies it, Cancel drops it).
    pub(super) event_log_filter: Option<super::event_log_filter_box::EventLogFilterBox>,
    /// The chat whose recent-actions poll is running (one timer task).
    pub(super) event_log_poll: Option<ChatId>,
    /// B7: group / channel settings dialog (topics, history, reactions,
    /// discussion group, ...).
    pub(super) group_settings_dialog: Option<GroupSettingsDialog>,
    /// Batch 8: the join-requests box of this chat (from the requests bar).
    pub(super) join_requests_dialog: Option<super::chat_bars::JoinRequestsDialog>,
    /// Slice G2: chat welcome-message editor.
    pub(super) welcome_dialog: Option<WelcomeDialog>,
    /// The group or channel Edit box (`group_edit_box/`).
    pub(super) group_edit_box: Option<super::group_edit_box::GroupEditBox>,
}

impl AdminUi {
    pub(super) fn new() -> Self {
        Self {
            invite_link_box: None,
            invite_link_details: None,
            revoked_links_open: false,
            admin_dialog: None,
            create_chat_dialog: None,
            community: CommunityUi::default(),
            member_dialog: None,
            permissions_dialog: None,
            add_special: None,
            username_dialog: None,
            restrict_dialog: None,
            ownership_dialog: None,
            group_confirm_dialog: None,
            forum_manage_dialog: None,
            event_log_search: None,
            event_log_list: super::event_log::new_event_log_list(),
            event_log_ids: std::cell::RefCell::new(Vec::new()),
            event_log_keystroke: 0,
            event_log_hooked: false,
            event_log_autoload: std::cell::Cell::new(0),
            event_log_filter: None,
            event_log_poll: None,
            group_settings_dialog: None,
            join_requests_dialog: None,
            welcome_dialog: None,
            group_edit_box: None,
        }
    }
}
