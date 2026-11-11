//! The TDLib 1.8.70 community screens, after tdesktop
//! (`boxes/peers/add_to_community_box.cpp`,
//! `info/community_requests/info_community_requests_widget.cpp`,
//! `boxes/moderate_messages_box.cpp`, the collapsed community row):
//! adding a chat to a community and removing it, the pending requests
//! list, "Show as One Chat" and its chat-list row, community
//! notifications and the community ban.
//!
//! One box shows at a time ([`CommunityBox`]); a box that leads to
//! another replaces it, and Cancel goes back to the box it came from.
//! Requests answer through `Session::communities` outcomes, drained in
//! [`QuillApp::drain_communities`].
#![deny(clippy::indexing_slicing)]

mod ban;
mod boxes;
mod chooser;
mod create;
mod demo;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod flow_tests;
mod info;
mod manage;
mod mute_for;
mod outcomes;
mod requests;
mod row;
mod row_menu;
#[cfg(test)]
mod tests;

use super::app::QuillApp;
use super::*;
use crate::ui::shell::QuillShell;
use crate::ui::status_note::StatusNote;
use crate::ui::user_error::user_error;
use gpui_kit::component::dialog::DialogContent;
use gpui_kit::component::input::InputState;
use gpui_kit::*;
use quill::community_row::CommunityRowSummary;
use quill::lang::{keys, tr, tr_with};
use quill::state::{CommunityChatKind, CommunityMember};
use std::time::Instant;

pub(crate) use requests::UndoAction;
pub(crate) use row_menu::RowMenu;

pub(crate) enum CommunityBox {
    /// tdesktop `ShowAddToCommunityBox`: a chat picks its community —
    /// create one, or one of the joined communities.
    Pick { chat_id: i64 },
    /// `ShowChooseChatToAddBox`: a community picks one of the person's
    /// chats.
    ChooseChat {
        community_id: i64,
        search: Entity<InputState>,
        /// Redraws the list as the search changes.
        _typed: Subscription,
    },
    /// tdesktop `CommunityIdentityBox`: a new community's photo and name,
    /// made around the chat that asked (`createCommunity`).
    Create { chat_id: i64, draft: NewCommunity },
    /// `ChooseVisibilityBox` for the new community's first chat; its
    /// confirm creates the community.
    CreateVisibility {
        chat_id: i64,
        draft: NewCommunity,
        hidden: bool,
    },
    /// `ChooseVisibilityBox`: Visible or Hidden, chosen once.
    Visibility {
        community_id: i64,
        chat_id: i64,
        hidden: bool,
        back: Option<Back>,
    },
    /// The person can't change the list: adding sends a request to the
    /// admins (`lng_community_add_confirm`).
    ConfirmAdd {
        community_id: i64,
        chat_id: i64,
        hidden: bool,
    },
    /// "Remove {group} from the community?"
    ConfirmRemove { community_id: i64, chat_id: i64 },
    Requests {
        community_id: i64,
        undo: Option<UndoAction>,
    },
    /// tdesktop `MessageGroupOwnerBox`: a hidden suggested chat can't be
    /// opened, so its box offers a message to the person who suggested
    /// it.
    HiddenChat {
        community_id: i64,
        chat_id: i64,
        owner_user_id: i64,
    },
    /// Add All / Decline All, confirmed.
    ConfirmProcessAll {
        community_id: i64,
        approve: bool,
        count: i32,
    },
    /// "Do you want to join {group}?" for a community chat the person is
    /// not in (`ShowCommunityChatJoinConfirm`).
    ConfirmJoin { chat_id: i64 },
    /// "Ungroup Chats" from the collapsed row's menu.
    Ungroup { community_id: i64 },
    /// "Mute for..." from the row menu's Mute submenu (tdesktop
    /// `MuteBox`): days, hours and minutes.
    MuteFor { community_id: i64 },
    /// `BanFromCommunityWithWarning`: the member owns chats in the
    /// community, which the ban removes.
    BanWarning {
        community_id: i64,
        member: CommunityMember,
        name: String,
        owned_chat_ids: Vec<i64>,
    },
}

/// What the identity box holds while the visibility box asks.
#[derive(Clone)]
pub(crate) struct NewCommunity {
    pub(crate) name: Entity<InputState>,
    /// The chosen picture, sent with `setCommunityPhoto` once the
    /// community exists, as tdesktop uploads it after `create`.
    pub(crate) photo: Option<std::path::PathBuf>,
}

/// Where Cancel on the visibility box returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Back {
    Pick { chat_id: i64 },
    ChooseChat { community_id: i64 },
}

/// A community ban waiting for `getCommunityMemberChats`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingBan {
    pub(crate) community_id: i64,
    pub(crate) member: CommunityMember,
    pub(crate) name: String,
    /// The box that was open when Ban was pressed (`open_dialog_name`):
    /// the warning opens only while it is still the one on screen.
    pub(crate) over: Option<&'static str>,
}

impl CommunitiesUi {
    /// Put `open` in place of the open box. An answer the Requests box
    /// still holds for its undo time comes back to be sent: it goes when
    /// the list closes, whatever replaces it.
    pub(crate) fn replace_box(&mut self, open: Option<CommunityBox>) -> Option<(i64, UndoAction)> {
        match std::mem::replace(&mut self.open, open) {
            Some(CommunityBox::Requests {
                community_id,
                undo: Some(undo),
            }) => Some((community_id, undo)),
            _ => None,
        }
    }
}

#[derive(Default)]
pub(crate) struct CommunitiesUi {
    pub(crate) open: Option<CommunityBox>,
    /// A ban waiting for the member's chats, with no box of its own
    /// (`drain_pending_ban`). A newer ban replaces it.
    pub(crate) ban_wait: Option<PendingBan>,
    /// "Create a New Chat" from this community's chooser: the new chat goes
    /// to the visibility box once made (`drain_new_community_chat`). Its own
    /// slot: starting it never drops a ban still waiting for its answer.
    pub(crate) new_chat_for: Option<i64>,
    pub(crate) row_menu: Option<RowMenu>,
    /// The pending requests list's scroll, for the community it shows.
    pub(crate) requests_view: RequestsView,
}

/// The pending requests list's scroll and paging, keyed by its community:
/// another community's list starts at the top and asks afresh, and Back
/// from a chat's details to the same list keeps its place
/// (`RequestsView::of`).
#[derive(Default)]
pub(crate) struct RequestsView {
    community_id: Option<i64>,
    pub(crate) scroll: ScrollHandle,
    pub(crate) asked: crate::ui::scroll_load::Asked,
}

impl RequestsView {
    /// The view of `community_id`'s list, made new for another community.
    pub(crate) fn of(&mut self, community_id: i64) -> &Self {
        if self.community_id != Some(community_id) {
            *self = Self {
                community_id: Some(community_id),
                ..Self::default()
            };
        }
        self
    }

    /// A fresh open of a list: from the top.
    pub(crate) fn reset(&mut self, community_id: i64) {
        *self = Self {
            community_id: Some(community_id),
            ..Self::default()
        };
    }
}

/// "1,200 members" / "5,300 subscribers" under a community chat.
fn member_status(kind: CommunityChatKind, count: i32) -> String {
    quill::lang::trn_decimal(
        if kind == CommunityChatKind::Channel {
            keys::CHAT_STATUS_SUBSCRIBERS
        } else {
            keys::CHAT_STATUS_MEMBERS
        },
        i64::from(count),
    )
}

fn by_kind(kind: CommunityChatKind, group: Key, channel: Key, bot: Key) -> &'static str {
    tr(match kind {
        CommunityChatKind::Group => group,
        CommunityChatKind::Channel => channel,
        CommunityChatKind::Bot => bot,
    })
}

use quill::lang::Key;

impl QuillApp {
    fn community_chat_kind(&self, chat_id: i64) -> CommunityChatKind {
        self.session()
            .and_then(|s| s.community_chat_kind(chat_id))
            .unwrap_or(CommunityChatKind::Group)
    }

    fn community_chat_title(&self, chat_id: i64) -> String {
        self.session()
            .and_then(|s| s.chats.get(&chat_id))
            .map(|chat| chat.title.clone())
            .unwrap_or_default()
    }

    fn community_name(&self, community_id: i64) -> String {
        self.session()
            .and_then(|s| s.groups.communities.get(&community_id))
            .map(|community| community.name.clone())
            .unwrap_or_else(|| tr(keys::COMMUNITY_TITLE).to_string())
    }

    /// Whether a community step's send went. `Ok(None)` is a refusal (the
    /// community or chat is gone, or the person lost access), told in a
    /// note: the step is never shown as done. The one exception is a step
    /// already out for a community still there, which is that step.
    pub(super) fn community_step_went(
        &mut self,
        community_id: Option<i64>,
        sent: Result<Option<quill::ids::RequestId>, quill::connect::ConnectSendError>,
    ) -> bool {
        match sent {
            Ok(Some(_)) => true,
            Ok(None) => {
                let accessible = community_id.is_some_and(|id| {
                    self.session()
                        .and_then(|s| s.groups.communities.get(&id))
                        .is_some_and(|community| community.have_access)
                });
                if !accessible {
                    self.set_note(StatusNote::error(tr(keys::Q_COMMUNITY_STEP_UNAVAILABLE)));
                }
                accessible
            }
            Err(err) => {
                self.set_note(StatusNote::error(user_error(&err)));
                false
            }
        }
    }

    pub(super) fn show_community_box(&mut self, open: CommunityBox, cx: &mut Context<Self>) {
        // box-host: every community box opens here, in the box's own kind
        // (`CommunityBox::kind`).
        self.open_box(open.kind());
        if let Some((community_id, undo)) = self.admin.community.flow.replace_box(Some(open)) {
            self.perform_request_action(community_id, undo, cx);
        }
        cx.notify();
    }

    pub(super) fn flush_and_close_community_box(&mut self, cx: &mut Context<Self>) {
        if let Some((community_id, undo)) = self.admin.community.flow.replace_box(None) {
            self.perform_request_action(community_id, undo, cx);
        }
        cx.notify();
    }

    /// The manage screen's "Add Group to a Community".
    pub(super) fn open_add_to_community(&mut self, chat_id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.fetch_joined_communities()
        {
            self.set_note(StatusNote::error(user_error(&err)));
        }
        self.show_community_box(CommunityBox::Pick { chat_id }, cx);
    }

    /// A community was picked for `chat_id`: choose the visibility.
    pub(super) fn choose_community_visibility(
        &mut self,
        community_id: i64,
        chat_id: i64,
        back: Option<Back>,
        cx: &mut Context<Self>,
    ) {
        self.show_community_box(
            CommunityBox::Visibility {
                community_id,
                chat_id,
                hidden: false,
                back,
            },
            cx,
        );
    }

    pub(super) fn community_visibility_back(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let back = match &self.admin.community.flow.open {
            Some(CommunityBox::Visibility { back, .. }) => *back,
            Some(CommunityBox::CreateVisibility { chat_id, draft, .. }) => {
                let (chat_id, draft) = (*chat_id, draft.clone());
                self.show_community_box(CommunityBox::Create { chat_id, draft }, cx);
                return;
            }
            _ => None,
        };
        match back {
            Some(Back::Pick { chat_id }) => {
                self.show_community_box(CommunityBox::Pick { chat_id }, cx)
            }
            Some(Back::ChooseChat { community_id }) => {
                self.open_choose_community_chat(community_id, window, cx)
            }
            None => self.flush_and_close_community_box(cx),
        }
    }

    /// "Add to Community" on the visibility box: add at once when the
    /// person can change the list, else confirm the request first.
    pub(super) fn submit_community_visibility(&mut self, cx: &mut Context<Self>) {
        if let Some(CommunityBox::CreateVisibility { .. }) = self.admin.community.flow.open {
            self.submit_new_community(cx);
            return;
        }
        let Some(CommunityBox::Visibility {
            community_id,
            chat_id,
            hidden,
            ..
        }) = self.admin.community.flow.open
        else {
            return;
        };
        let can_edit = self
            .session()
            .and_then(|s| s.groups.communities.get(&community_id))
            .is_some_and(|community| community.can_edit_chat_list);
        if can_edit {
            self.send_add_community_chat(community_id, chat_id, hidden, cx);
        } else {
            self.show_community_box(
                CommunityBox::ConfirmAdd {
                    community_id,
                    chat_id,
                    hidden,
                },
                cx,
            );
        }
    }

    pub(super) fn send_add_community_chat(
        &mut self,
        community_id: i64,
        chat_id: i64,
        hidden: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            let sent = live
                .driver
                .add_community_chat(community_id, chat_id, hidden);
            self.community_step_went(Some(community_id), sent);
        }
        cx.notify();
    }

    /// The manage screen's "Remove Group from Community".
    pub(super) fn ask_remove_from_community(
        &mut self,
        community_id: i64,
        chat_id: i64,
        cx: &mut Context<Self>,
    ) {
        self.show_community_box(
            CommunityBox::ConfirmRemove {
                community_id,
                chat_id,
            },
            cx,
        );
    }

    /// Confirmed: tdesktop closes the box at once and toasts on the
    /// answer.
    pub(super) fn confirm_remove_from_community(&mut self, cx: &mut Context<Self>) {
        let Some(CommunityBox::ConfirmRemove {
            community_id,
            chat_id,
        }) = self.admin.community.flow.open.take()
        else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            let sent = live.driver.remove_community_chat(community_id, chat_id);
            self.community_step_went(Some(community_id), sent);
        }
        cx.notify();
    }

    /// "Show as One Chat" switch.
    pub(super) fn set_community_collapsed(
        &mut self,
        community_id: i64,
        collapsed: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live
                .driver
                .toggle_community_chats_collapsed(community_id, collapsed)
        {
            self.set_note(StatusNote::error(user_error(&err)));
        }
        cx.notify();
    }

    /// The collapsed rows of the main list, or none outside it.
    pub(super) fn listed_community_rows(&self) -> Vec<CommunityRowSummary> {
        self.session()
            .map(|session| session.collapsed_community_rows())
            .unwrap_or_default()
    }

    /// Ban `member` from the community: ask which of its chats they own
    /// first (`BanFromCommunityWithWarning`).
    pub(in crate::ui) fn start_community_ban(
        &mut self,
        community_id: i64,
        member: CommunityMember,
        name: String,
        cx: &mut Context<Self>,
    ) {
        let over = QuillShell::open_dialog_name(self);
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let sent = live
            .driver
            .fetch_community_member_chats(community_id, member);
        let asking = ban::answer_coming(&live.driver.session, community_id, member);
        match ban::ban_start(sent, asking) {
            ban::BanStart::Wait => {
                self.admin.community.flow.ban_wait = Some(PendingBan {
                    community_id,
                    member,
                    name,
                    over,
                });
            }
            ban::BanStart::Refused => {
                self.set_note(StatusNote::error(tr(keys::Q_COMMUNITY_BAN_UNAVAILABLE)))
            }
            ban::BanStart::Failed(err) => self.set_note(StatusNote::error(user_error(&err))),
        }
        cx.notify();
    }
}
