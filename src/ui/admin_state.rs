//! Group and channel management boxes: invite links, admins, members, permissions.
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::ids::ChatId;

/// The recent-actions search box and whose stored query it shows: it is
/// refilled whenever the open log's chat is another one.
pub(crate) struct EventLogSearch {
    pub(super) input: Entity<TextareaState>,
    pub(super) shows: Option<ChatId>,
}

pub(crate) struct AdminUi {
    /// The New Link / Edit Link box (`invite_link_box/`).
    pub(super) invite_link_box: Option<super::invite_link_box::InviteLinkBox>,
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
    pub(super) event_log_search: Option<EventLogSearch>,
    /// The recent-actions list (oldest event first, newest at the bottom):
    /// earlier events load when it is scrolled to its top.
    pub(super) event_log_list: ListState,
    /// The event ids the list was last built for (`event_log_list::plan_sync`).
    pub(super) event_log_ids: std::cell::Cell<Vec<i64>>,
    /// The recent-actions page last opened, with the timers that belong to
    /// it. Opening it for another chat replaces it, and its timers stop.
    pub(super) event_log_page: Option<EventLogPage>,
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

/// The recent-actions page of one chat and what runs for it. The timer
/// tasks are owned here: replacing the page or dropping the screen stops
/// them, and a newer one replaces an older one of its kind.
pub(super) struct EventLogPage {
    pub(super) chat: ChatId,
    /// Where the automatic "load earlier" stands in this page (back to
    /// `Idle` when the list starts over).
    pub(super) autoload: std::cell::Cell<OlderAsk>,
    /// The search debounce: each edit replaces it, so only the last one
    /// searches.
    pub(super) typed: Option<gpui_kit::Task<()>>,
    /// The poll timer, which ends by itself when the page closes.
    pub(super) poll: Option<gpui_kit::Task<()>>,
}

/// The "load earlier events" ask of one recent-actions page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum OlderAsk {
    /// Nothing asked yet: a list scrolled to its top asks.
    #[default]
    Idle,
    /// Asked at this list length; the same length does not ask again.
    Asked(usize),
    /// The send was refused (no connection): the list shows Retry and
    /// asks nothing until it is pressed or the list starts over.
    Refused,
}

impl OlderAsk {
    /// Whether a list of `len` events may ask for earlier ones now.
    pub(super) fn may_ask(self, len: usize) -> bool {
        match self {
            Self::Idle => true,
            Self::Asked(asked) => asked != len,
            Self::Refused => false,
        }
    }
}

impl EventLogPage {
    pub(super) fn new(chat: ChatId) -> Self {
        Self {
            chat,
            autoload: std::cell::Cell::new(OlderAsk::Idle),
            typed: None,
            poll: None,
        }
    }
}

/// The boost box keeps its state in the session (`groups.boost_box`),
/// which goes to the background with the account: it closes here so it
/// does not come back on switch-back. So do the invite-link pages the
/// Group Settings box opened there (`drop_invite_link_pages`). A Pending
/// Requests Add or Decline still in its undo time is sent now, through the
/// leaving account's client, as closing the list does
/// (`flush_and_close_community_box`): the screen's replacement drops its
/// timer, and the answer would be lost.
impl crate::ui::on_screen::Farewell for AdminUi {
    fn farewell(
        app: &mut QuillApp,
        _: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<QuillApp>,
    ) {
        app.flush_and_close_community_box(cx);
        if app
            .session()
            .is_some_and(|session| session.groups.boost_box.is_some())
        {
            app.close_boost_box_state(cx);
        }
        app.drop_invite_link_pages();
    }
}

impl AdminUi {
    pub(super) fn new() -> Self {
        Self {
            invite_link_box: None,
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
            event_log_ids: std::cell::Cell::new(Vec::new()),
            event_log_page: None,
            group_settings_dialog: None,
            join_requests_dialog: None,
            welcome_dialog: None,
            group_edit_box: None,
        }
    }
}
