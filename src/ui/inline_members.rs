//! The members block of the group info panel: which group it shows, the
//! search box, paging as the list scrolls and refetching after membership
//! changes. The drawing is in `inline_members_view.rs`.

use super::app::QuillApp;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::*;
use quill::ids::ChatId;
use quill::inline_members::{MoreAction, next_more};
use quill::state::{InfoPanelTarget, MemberListFilter, SupergroupMembersFetch};
use quill::telegram::envelope::ChatKind;
use std::time::Duration;

/// Wait after the last keystroke before asking the server to search.
const SEARCH_DELAY: Duration = Duration::from_millis(300);
/// Rows are asked for when this close to the end of the list.
const PREFETCH: f32 = 160.;

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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(shape) = self.inline_members_shape(target) else {
            return;
        };
        if self.inline_members.search.is_none() {
            let input = cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("Search members")
                    .auto_grow(1, 1)
                    .submit_on_enter(false)
            });
            cx.subscribe_in(
                &input,
                window,
                |this, state, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        let text = state.read(cx).value().to_string();
                        this.inline_members_changed(&text, cx);
                    }
                },
            )
            .detach();
            self.inline_members.search = Some(input);
        }
        if self.inline_members.chat != Some(shape.chat) {
            self.inline_members.reset(Some(shape.chat));
            if let Some(input) = self.inline_members.search.clone() {
                input.update(cx, |state, cx| state.set_value("", window, cx));
            }
        }
        self.inline_members_fetch(shape, cx);
        cx.notify();
    }

    /// Ask for the page the block shows (deduped by the driver).
    fn inline_members_fetch(&mut self, shape: GroupShape, cx: &mut Context<Self>) {
        let query = self.inline_members.query.clone();
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let result = if shape.is_basic {
            live.driver
                .fetch_basic_group_members(shape.chat)
                .map(|_| ())
        } else if query.is_empty() {
            live.driver
                .fetch_supergroup_members(shape.chat, MemberListFilter::Recent, "")
                .map(|_| ())
        } else {
            live.driver
                .refresh_supergroup_members(shape.chat, MemberListFilter::Search, &query)
                .map(|_| ())
        };
        if result.is_err() {
            self.connection.status_note = "could not load members".into();
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

    /// The search text changed: narrow the list at once (basic groups) or
    /// after a short pause (server search for supergroups).
    pub(super) fn inline_members_changed(&mut self, text: &str, cx: &mut Context<Self>) {
        let query = text.trim().to_string();
        if query == self.inline_members.query {
            return;
        }
        self.inline_members.query = query.clone();
        self.inline_members.token += 1;
        self.inline_members.shown = quill::inline_members::ROWS_STEP;
        self.inline_members.failed_offset = None;
        self.inline_members.scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();
        let Some(shape) = self.inline_members_current() else {
            return;
        };
        if shape.is_basic {
            return;
        }
        if query.is_empty() {
            self.inline_members_fetch(shape, cx);
            return;
        }
        let token = self.inline_members.token;
        self.arm_inline_members_search(token, cx);
    }

    fn arm_inline_members_search(&mut self, token: u64, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SEARCH_DELAY).await;
            let _ = this.update(cx, |this, cx| this.run_inline_members_search(token, cx));
        })
        .detach();
    }

    fn run_inline_members_search(&mut self, token: u64, cx: &mut Context<Self>) {
        if token != self.inline_members.token {
            return;
        }
        let Some(shape) = self.inline_members_current() else {
            return;
        };
        // An earlier search is still running: look again once it is done,
        // then send the latest text.
        let busy = self.session().is_some_and(|session| {
            matches!(
                session
                    .groups
                    .supergroup_members
                    .get(&(shape.chat.0, MemberListFilter::Search)),
                Some(SupergroupMembersFetch::Loading)
            )
        });
        if busy {
            self.arm_inline_members_search(token, cx);
            return;
        }
        self.inline_members_fetch(shape, cx);
        cx.notify();
    }

    /// Show or hide the search box; hiding clears the query.
    pub(super) fn toggle_inline_members_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.inline_members.search_open = !self.inline_members.search_open;
        if !self.inline_members.search_open {
            if let Some(input) = self.inline_members.search.clone() {
                input.update(cx, |state, cx| state.set_value("", window, cx));
            }
            self.inline_members_changed("", cx);
        } else if let Some(input) = self.inline_members.search.clone() {
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
        let Some((loaded, total, filter)) = self.inline_members_counts(shape) else {
            return;
        };
        let shown = self.inline_members.shown;
        if shown >= loaded && loaded as i32 >= total {
            return;
        }
        let handle = &self.inline_members.scroll;
        // Not laid out yet: the next render asks again.
        if handle.bounds().size.height <= px(0.) {
            return;
        }
        let left = handle.max_offset().y + handle.offset().y;
        if left > px(PREFETCH) {
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
                let query = self.inline_members.query.clone();
                if let Some(live) = self.live.as_mut() {
                    let _ = live
                        .driver
                        .fetch_supergroup_members_more(shape.chat, filter, &query, offset);
                }
                cx.notify();
            }
        }
    }

    /// (members loaded, total on the server, filter) of the list shown.
    fn inline_members_counts(&self, shape: GroupShape) -> Option<(usize, i32, MemberListFilter)> {
        let session = self.session()?;
        let filter = self.inline_members_filter();
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

    /// Which cached page backs the list: recent members, or the search.
    pub(super) fn inline_members_filter(&self) -> MemberListFilter {
        if self.inline_members.query.is_empty() {
            MemberListFilter::Recent
        } else {
            MemberListFilter::Search
        }
    }
}
