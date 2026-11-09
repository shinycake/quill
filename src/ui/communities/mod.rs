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
mod demo;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod flow_tests;
mod info;
mod manage;
mod outcomes;
mod requests;
mod row;
pub(in crate::ui) use row::community_avatar_centered;
#[cfg(test)]
mod tests;

use super::app::QuillApp;
use super::*;
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
    /// `BanFromCommunityWithWarning`: the member owns chats in the
    /// community, which the ban removes.
    BanWarning {
        community_id: i64,
        member: CommunityMember,
        name: String,
        owned_chat_ids: Vec<i64>,
    },
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
}

#[derive(Default)]
pub(crate) struct CommunitiesUi {
    pub(crate) open: Option<CommunityBox>,
    pub(crate) pending_ban: Option<PendingBan>,
    /// "Create a New Chat" from this community's chooser: the new chat
    /// goes to the visibility box once made.
    pub(crate) awaiting_new_chat: Option<i64>,
    pub(crate) row_menu: Option<(i64, Point<Pixels>)>,
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

    pub(super) fn show_community_box(&mut self, open: CommunityBox, cx: &mut Context<Self>) {
        // box-host: every community box opens here, in the box's own kind
        // (`CommunityBox::kind`).
        self.open_box(open.kind());
        self.admin.community.flow.open = Some(open);
        cx.notify();
    }

    pub(super) fn flush_and_close_community_box(&mut self, cx: &mut Context<Self>) {
        if let Some(CommunityBox::Requests {
            community_id,
            undo: Some(undo),
        }) = self.admin.community.flow.open.take()
        {
            self.perform_request_action(community_id, undo, cx);
        }
        cx.notify();
    }

    /// The manage screen's "Add Group to a Community".
    pub(super) fn open_add_to_community(&mut self, chat_id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.fetch_joined_communities()
        {
            self.connection
                .set_note(StatusNote::error(user_error(&err)));
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
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live
                .driver
                .add_community_chat(community_id, chat_id, hidden)
        {
            self.connection
                .set_note(StatusNote::error(user_error(&err)));
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
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.remove_community_chat(community_id, chat_id)
        {
            self.connection
                .set_note(StatusNote::error(user_error(&err)));
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
            self.connection
                .set_note(StatusNote::error(user_error(&err)));
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
        let Some(live) = self.live.as_mut() else {
            return;
        };
        match live
            .driver
            .fetch_community_member_chats(community_id, member)
        {
            Ok(_) => {
                self.admin.community.flow.pending_ban = Some(PendingBan {
                    community_id,
                    member,
                    name,
                });
            }
            Err(err) => self
                .connection
                .set_note(StatusNote::error(user_error(&err))),
        }
        cx.notify();
    }
}
