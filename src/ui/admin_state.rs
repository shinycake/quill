//! Group and channel management boxes: invite links, admins, members, permissions.
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::ids::ChatId;

/// The recent-actions search box of one page: it shows that chat's stored
/// query from the moment it is built, and goes with the page.
pub(crate) struct EventLogSearch {
    pub(super) input: Entity<TextareaState>,
    /// Typing in the box searches once it pauses; held so the page's end
    /// ends the subscription.
    pub(super) _typed: Subscription,
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
    /// Unbans and "Remove user" bans sent from the member boxes and not
    /// answered yet.
    pub(super) member_changes: Vec<super::group_moderation::MemberChange>,
    /// Slice G1: public username editor.
    pub(super) username_dialog: Option<UsernameDialog>,
    /// Slice G1: restrict/ban dialog.
    pub(super) restrict_dialog: Option<RestrictDialog>,
    pub(super) ownership_dialog: Option<OwnershipDialog>,
    /// Slice G1: delete / leave / broadcast-upgrade / ban confirmations.
    pub(super) group_confirm_dialog: Option<GroupConfirmDialog>,
    /// Slice G2: forum-topic management dialog.
    pub(super) forum_manage_dialog: Option<ForumManageDialog>,
    /// The recent-actions list with the event ids it shows.
    pub(super) event_log_rows: super::event_log::EventLogRows,
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
    /// The page's search box, built once the window it shows in is known.
    pub(super) search: Option<EventLogSearch>,
    /// The list length an automatic "load earlier" was last asked for in
    /// this page (cleared when the list starts over).
    pub(super) autoload: std::cell::Cell<Option<usize>>,
    /// The search debounce: each edit replaces it, so only the last one
    /// searches.
    pub(super) typed: SearchPause,
    /// The poll timer, which ends by itself when the page closes or the app
    /// goes inactive (`resume_event_log_poll` starts it again).
    pub(super) poll: EventLogPoll,
}

/// The page's search debounce: not waiting, or waiting on the timer it
/// owns. The timer's own firing sets `Idle` (`fire`), so a finished task is
/// never read as a pending one.
#[derive(Default)]
pub(super) enum SearchPause {
    #[default]
    Idle,
    Waiting {
        _timer: gpui_kit::Task<()>,
    },
}

impl SearchPause {
    /// Wait for `timer` (replacing, and so cancelling, an earlier one).
    pub(super) fn arm(&mut self, timer: gpui_kit::Task<()>) {
        *self = Self::Waiting { _timer: timer };
    }

    /// The timer ended: whether a search was waiting for it (once).
    pub(super) fn fire(&mut self) -> bool {
        matches!(std::mem::take(self), Self::Waiting { .. })
    }
}

/// Whether the page's poll runs. The loop's own exit sets `Stopped`, so a
/// finished task is never read as a running one.
#[derive(Default)]
pub(super) enum EventLogPoll {
    #[default]
    Stopped,
    Running {
        _task: gpui_kit::Task<()>,
    },
}

impl EventLogPage {
    pub(super) fn new(chat: ChatId) -> Self {
        Self {
            chat,
            search: None,
            autoload: std::cell::Cell::new(None),
            typed: SearchPause::Idle,
            poll: EventLogPoll::Stopped,
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
            member_changes: Vec::new(),
            username_dialog: None,
            restrict_dialog: None,
            ownership_dialog: None,
            group_confirm_dialog: None,
            forum_manage_dialog: None,
            event_log_rows: super::event_log::EventLogRows::new(),
            event_log_page: None,
            group_settings_dialog: None,
            join_requests_dialog: None,
            welcome_dialog: None,
            group_edit_box: None,
        }
    }
}

#[cfg(test)]
mod search_pause_tests {
    use super::SearchPause;

    /// The debounce fires once per arming, and a later arming replaces the
    /// earlier one.
    #[test]
    fn a_search_pause_fires_once() {
        let mut pause = SearchPause::default();
        assert!(!pause.fire(), "nothing was typed");
        pause.arm(gpui_kit::Task::ready(()));
        pause.arm(gpui_kit::Task::ready(()));
        assert!(pause.fire());
        assert!(!pause.fire(), "a fired pause is idle, not a finished task");
    }
}
