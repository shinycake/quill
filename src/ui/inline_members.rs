//! The members block of the group info panel: which group it shows, the
//! search box, paging as the list scrolls and refetching after membership
//! changes. The drawing is in `inline_members_view.rs`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use crate::ui::scroll_load::scrolled_near_end;
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::inline_members::{MoreAction, next_more};
use quill::lang::{keys, tr};
use quill::state::{InfoPanelTarget, MemberListFilter, SupergroupMembersFetch};
use quill::telegram::envelope::ChatKind;

/// What kind of group the info panel is showing.
#[derive(Clone, Copy)]
pub(super) struct GroupShape {
    pub(super) chat: ChatId,
    pub(super) is_basic: bool,
    pub(super) is_channel: bool,
}

impl QuillApp {
    /// The group behind an info panel target; `None` for other panels.
    pub(super) fn inline_members_shape(&self, target: InfoPanelTarget) -> Option<GroupShape> {
        let session = self.session()?;
        session
            .chats
            .values()
            .find_map(|chat| match (chat.kind.clone(), target) {
                (ChatKind::BasicGroup { basic_group_id }, InfoPanelTarget::BasicGroup(id))
                    if basic_group_id == id =>
                {
                    Some(GroupShape {
                        chat: chat.id,
                        is_basic: true,
                        is_channel: false,
                    })
                }
                (
                    ChatKind::Supergroup {
                        supergroup_id,
                        is_channel,
                    },
                    InfoPanelTarget::Supergroup(id),
                ) if supergroup_id == id => Some(GroupShape {
                    chat: chat.id,
                    is_basic: false,
                    is_channel,
                }),
                _ => None,
            })
    }

    /// The members block for a group info panel, if the target is a group.
    pub(in crate::ui) fn inline_members_block(
        &self,
        target: InfoPanelTarget,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let shape = self.inline_members_shape(target)?;
        self.inline_members_section(shape, cx)
    }

    /// The open info panel's group, if it is one.
    fn inline_members_current(&self) -> Option<GroupShape> {
        let target = self.session()?.users_state.open_info_panel?;
        self.inline_members_shape(target)
    }

    /// An info panel opened: point the block at its group, start from the
    /// first page and ask for it.
    pub(super) fn inline_members_opened(
        &mut self,
        target: InfoPanelTarget,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(shape) = self.inline_members_shape(target) else {
            return;
        };
        if self.inline_members.chat != Some(shape.chat) {
            self.inline_members.reset(Some(shape.chat));
        }
        self.inline_members_fetch(shape, cx);
        cx.notify();
    }

    /// Ask for the page the block shows (deduped by the driver).
    fn inline_members_fetch(&mut self, shape: GroupShape, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let result = if shape.is_basic {
            live.driver
                .fetch_basic_group_members(shape.chat)
                .map(|_| ())
        } else {
            live.driver
                .fetch_supergroup_members(shape.chat, MemberListFilter::Recent, "")
                .map(|_| ())
        };
        if result.is_err() {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_MEDIA_COULD_NOT_LOAD_MEMBERS)));
            cx.notify();
        }
    }

    /// Membership changed (`updateChatMember` dropped the cached pages):
    /// fetch again if the panel is showing that group.
    pub(super) fn inline_members_stale(&mut self, stale: &[i64], cx: &mut Context<Self>) {
        let Some(shape) = self.inline_members_current() else {
            return;
        };
        if stale.contains(&shape.chat.0) {
            self.inline_members.failed_offset = None;
            self.inline_members_fetch(shape, cx);
            cx.notify();
        }
    }

    /// The search button: the member dialog with its search box focused
    /// (tdesktop opens its full members section with search started).
    pub(super) fn inline_members_search(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_member_dialog(chat_id, window, cx);
        if let Some(input) = self
            .admin
            .member_dialog
            .as_ref()
            .map(|dialog| dialog.search_input.clone())
        {
            input.update(cx, |state, cx| state.focus(window, cx));
        }
        cx.notify();
    }

    /// "Add members" in the header: the member dialog with its add section
    /// open.
    pub(super) fn inline_members_add(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_member_dialog(chat_id, window, cx);
        if let Some(dialog) = self.admin.member_dialog.as_mut() {
            dialog.add_open = true;
        }
        cx.notify();
    }

    /// Called when the list may need more rows: near its end, draw the
    /// rows already loaded, then ask the server for the next page.
    pub(super) fn maybe_load_more_inline(&mut self, cx: &mut Context<Self>) {
        let Some(shape) = self.inline_members_current() else {
            return;
        };
        // Members hidden from the viewer: show the administrators.
        if self.inline_members_filter(shape) == MemberListFilter::Administrators
            && let Some(live) = self.live.as_mut()
        {
            let _ = live.driver.fetch_supergroup_members(
                shape.chat,
                MemberListFilter::Administrators,
                "",
            );
        }
        let Some((loaded, total, filter)) = self.inline_members_counts(shape) else {
            return;
        };
        let shown = self.inline_members.shown;
        if shown >= loaded && loaded as i32 >= total {
            return;
        }
        // Not laid out yet: the next render asks again.
        if !scrolled_near_end(&self.inline_members.scroll) {
            return;
        }
        let in_flight = self.live.as_ref().is_some_and(|live| {
            live.driver
                .supergroup_members_more_in_flight(shape.chat, filter)
        });
        let action = next_more(
            shown,
            loaded,
            total,
            in_flight,
            self.inline_members.failed_offset,
        );
        match action {
            MoreAction::Nothing => {}
            MoreAction::Reveal(count) => {
                self.inline_members.shown = count;
                cx.notify();
            }
            MoreAction::Fetch { offset } => {
                if shape.is_basic {
                    return;
                }
                self.inline_members.failed_offset = Some(offset);
                if let Some(live) = self.live.as_mut() {
                    let _ = live
                        .driver
                        .fetch_supergroup_members_more(shape.chat, filter, "", offset);
                }
                cx.notify();
            }
        }
    }

    /// (members loaded, total on the server, filter) of the list shown.
    fn inline_members_counts(&self, shape: GroupShape) -> Option<(usize, i32, MemberListFilter)> {
        let session = self.session()?;
        let filter = self.inline_members_filter(shape);
        let fetch = if shape.is_basic {
            session.groups.basic_group_members.get(&shape.chat.0)
        } else {
            session
                .groups
                .supergroup_members
                .get(&(shape.chat.0, filter))
        };
        match fetch {
            Some(SupergroupMembersFetch::Loaded {
                members,
                total_count,
            }) => Some((members.len(), *total_count, filter)),
            _ => None,
        }
    }

    /// Which cached page backs the list: recent members, or the
    /// administrators when the viewer may not see the members (tdesktop
    /// shows "Administrators" then).
    pub(super) fn inline_members_filter(&self, shape: GroupShape) -> MemberListFilter {
        let recent_failed = self.session().is_some_and(|session| {
            matches!(
                session
                    .groups
                    .supergroup_members
                    .get(&(shape.chat.0, MemberListFilter::Recent)),
                Some(SupergroupMembersFetch::Failed(_))
            )
        });
        if !shape.is_basic && recent_failed {
            MemberListFilter::Administrators
        } else {
            MemberListFilter::Recent
        }
    }
}
