//! Group and channel management boxes: invite links, admins, members, permissions.

use super::*;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::ids::ChatId;
use std::sync::Arc;

pub(crate) struct AdminUi {
    /// Phase D3a: invite-link create dialog state.
    pub(super) invite_link_dialog: Option<InviteLinkDialog>,
    /// B8: the invite link whose "who joined" details are expanded.
    pub(super) invite_link_details: Option<(ChatId, String)>,
    /// B8: whether the revoked-links list is expanded.
    pub(super) revoked_links_open: bool,
    /// The invite link whose QR code is showing.
    pub(super) invite_link_qr: Option<(ChatId, String, Arc<RenderImage>)>,
    /// Phase D3b: admin-management dialog state (promote picker /
    /// rights editor / demote confirm).
    pub(super) admin_dialog: Option<AdminDialog>,
    /// Slice G1: group/supergroup/channel creation dialog.
    pub(super) create_chat_dialog: Option<CreateChatDialog>,
    /// Slice G10: communities dialog state (create dialog + hub flag).
    pub(super) community: CommunityUi,
    /// Slice G1: member-management dialog (tabs + add section).
    pub(super) member_dialog: Option<MemberDialog>,
    /// Slice G1: default chat permissions editor.
    pub(super) permissions_dialog: Option<PermissionsDialog>,
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
    /// B7: group / channel settings dialog (topics, history, reactions,
    /// discussion group, ...).
    pub(super) group_settings_dialog: Option<GroupSettingsDialog>,
    /// Batch 8: the join-requests box of this chat (from the requests bar).
    pub(super) join_requests_dialog: Option<super::chat_bars::JoinRequestsDialog>,
    /// Slice G2: chat welcome-message editor.
    pub(super) welcome_dialog: Option<WelcomeDialog>,
}

impl AdminUi {
    pub(super) fn new() -> Self {
        Self {
            invite_link_dialog: None,
            invite_link_details: None,
            revoked_links_open: false,
            invite_link_qr: None,
            admin_dialog: None,
            create_chat_dialog: None,
            community: CommunityUi::default(),
            member_dialog: None,
            permissions_dialog: None,
            username_dialog: None,
            restrict_dialog: None,
            ownership_dialog: None,
            group_confirm_dialog: None,
            forum_manage_dialog: None,
            event_log_search: None,
            group_settings_dialog: None,
            join_requests_dialog: None,
            welcome_dialog: None,
        }
    }
}
