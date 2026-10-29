//! Drawing of the members block in the group info panel: header with the
//! count, search and add buttons, then the rows (online first, with owner
//! and admin pills) in a list that pages as it scrolls.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_row::chat_avatar;
use super::inline_members::GroupShape;
use super::inline_members_state::MembersSnapshot;
use crate::ui::lean_buttons::{ghost_icon_button_untitled, ghost_text_button};
use gpui_kit::assets::IconName;
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::inline_members::{
    member_badge, members_title, online_sort_value, search_available, sort_online_first,
    sorts_by_online,
};
use quill::lang::{keys, tr, tr_with};
use quill::local_path::sandboxed_display_path;
use quill::state::{InfoPanelTarget, MemberListFilter, SupergroupMembersFetch};
use quill::telegram::envelope::{ChatKind, MessageSender, ParsedChatMember, UserStatusKind};

/// One member with what the list needs from the user cache.
struct MemberRow {
    member: ParsedChatMember,
    user_id: Option<i64>,
    name: String,
    status: UserStatusKind,
    is_bot: bool,
}

/// The rows of the list and where they stand.
struct MembersView {
    rows: Vec<MemberRow>,
    total: i32,
    loading: bool,
    failed: Option<String>,
    /// The members are hidden from the viewer: this is the admin list.
    admins_only: bool,
}

impl QuillApp {
    /// The members block for the open group info panel, `None` when the
    /// group has no member list to show (a channel that hides it).
    pub(in crate::ui) fn inline_members_section(
        &self,
        shape: GroupShape,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let chat_id = shape.chat;
        let view = self.inline_members_view(shape);
        if view.failed.is_some() && shape.is_channel && view.rows.is_empty() {
            return None;
        }
        let group_total = if shape.is_basic {
            view.total
        } else {
            match target_member_count(self, chat_id) {
                Some(count) if count > 0 => count,
                _ => view.total,
            }
        };
        let admins_only = view.admins_only;
        let can_add = session.chat_can_add_members(chat_id);
        let roots = self.media_display_roots();

        let mut header = div().flex().items_center().justify_between().px_2().child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(cx.theme().muted_foreground)
                .child(if admins_only {
                    tr(keys::Q_MEDIA_ADMINISTRATORS).to_string()
                } else {
                    members_title(group_total, shape.is_channel)
                }),
        );
        let mut buttons = div().flex().items_center().gap_1();
        if search_available(group_total) && !admins_only {
            buttons = buttons.child(ghost_icon_button_untitled(
                "inline-members-search",
                IconName::Search,
                tr(keys::Q_MEDIA_SEARCH_MEMBERS),
                cx.listener(move |this, _, window, cx| {
                    this.inline_members_search(chat_id, window, cx);
                }),
            ));
        }
        if can_add {
            buttons = buttons.child(ghost_icon_button_untitled(
                "inline-members-add",
                IconName::UserPlus,
                tr(keys::CHANNEL_ADD_MEMBERS),
                cx.listener(move |this, _, window, cx| {
                    this.inline_members_add(chat_id, window, cx);
                }),
            ));
        }
        header = header.child(buttons);

        let mut block = div()
            .flex()
            .flex_col()
            .w_full()
            .gap_1()
            .pt_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(header);

        let shown = self.inline_members.shown;
        let mut list = div()
            .id("inline-members-list")
            .flex()
            .flex_col()
            .w_full()
            .max_h(px(440.))
            .overflow_y_scroll()
            .track_scroll(&self.inline_members.scroll)
            .on_scroll(cx.listener(|this, _, _, cx| this.maybe_load_more_inline(cx)));
        let muted = cx.theme().muted_foreground;
        let hint = move |text: String| {
            div()
                .px_2()
                .py_1()
                .text_sm()
                .text_color(muted)
                .child(text)
                .into_any_element()
        };
        if let Some(message) = view.failed.clone().filter(|_| view.rows.is_empty()) {
            list = list.child(hint(message)).child(ghost_text_button(
                "inline-members-retry",
                tr(keys::Q_MEDIA_RETRY),
                cx.listener(move |this, _, _, cx| {
                    this.inline_members_stale(&[chat_id.0], cx);
                }),
            ));
        } else if view.rows.is_empty() {
            list = list.child(hint(if view.loading {
                tr(keys::Q_MEDIA_LOADING_MEMBERS).to_string()
            } else {
                tr(keys::Q_MEDIA_NO_MEMBERS).to_string()
            }));
        }
        for row in view.rows.iter().take(shown) {
            list = list.child(self.inline_member_row(chat_id, row, &roots, cx));
        }
        if view.loading && !view.rows.is_empty() {
            list = list.child(hint(tr(keys::Q_MEDIA_LOADING).to_string()));
        }
        block = block.child(list);

        // Look again once this frame is laid out: a short list or a
        // page that arrived may leave the end in view.
        let entity = cx.entity();
        cx.defer(move |cx| {
            entity.update(cx, |this, cx| this.maybe_load_more_inline(cx));
        });
        Some(block.into_any_element())
    }

    /// The rows for the group, from the cache; the last page stays up
    /// while a membership change refetches it.
    fn inline_members_view(&self, shape: GroupShape) -> MembersView {
        let empty = MembersView {
            rows: Vec::new(),
            total: 0,
            loading: true,
            failed: None,
            admins_only: false,
        };
        let Some(session) = self.session() else {
            return empty;
        };
        let filter = self.inline_members_filter(shape);
        let admins_only = filter == MemberListFilter::Administrators;
        let fetch = if shape.is_basic {
            session.groups.basic_group_members.get(&shape.chat.0)
        } else {
            session
                .groups
                .supergroup_members
                .get(&(shape.chat.0, filter))
        };
        let (members, total, loading, failed) = match fetch {
            Some(SupergroupMembersFetch::Loaded {
                members,
                total_count,
            }) => {
                if !admins_only {
                    self.inline_members.snapshot.replace(Some(MembersSnapshot {
                        chat: shape.chat,
                        members: members.clone(),
                        total: *total_count,
                    }));
                }
                (members.clone(), *total_count, false, None)
            }
            Some(SupergroupMembersFetch::Failed(message)) => {
                (Vec::new(), 0, false, Some(message.clone()))
            }
            Some(SupergroupMembersFetch::Loading) | None => {
                let snapshot = self.inline_members.snapshot.borrow();
                match snapshot
                    .as_ref()
                    .filter(|s| s.chat == shape.chat && !admins_only)
                {
                    Some(last) => (last.members.clone(), last.total, true, None),
                    None => (Vec::new(), 0, true, None),
                }
            }
        };
        let mut rows: Vec<MemberRow> = members
            .into_iter()
            .map(|member| {
                let user_id = match member.member_id {
                    MessageSender::User { user_id } => Some(user_id),
                    MessageSender::Chat { .. } => None,
                };
                let user = user_id.and_then(|id| session.user(id));
                let name = match (user, member.member_id) {
                    (Some(user), _) => user.display_name(),
                    (None, MessageSender::User { user_id }) => {
                        tr_with(keys::Q_MEDIA_USER, &[("user_id", &user_id.to_string())])
                    }
                    (None, MessageSender::Chat { chat_id }) => {
                        tr_with(keys::Q_MEDIA_CHANNEL, &[("chat_id", &chat_id.to_string())])
                    }
                };
                MemberRow {
                    user_id,
                    status: user.map(|u| u.status).unwrap_or_default(),
                    is_bot: user.is_some_and(|u| u.is_bot),
                    name,
                    member,
                }
            })
            .collect();
        let count = if shape.is_basic {
            total
        } else {
            target_member_count(self, shape.chat).unwrap_or(total)
        };
        if sorts_by_online(shape.is_basic, shape.is_channel, count) {
            let now = quill::local_time::now_unix();
            sort_online_first(&mut rows, |row| {
                online_sort_value(&row.status, row.is_bot, now)
            });
        }
        MembersView {
            rows,
            total,
            loading,
            failed,
            admins_only,
        }
    }

    /// One row: avatar, name, presence line and the owner/admin pill.
    /// Click opens the member's profile; the context menu is the one of
    /// the member dialog (mention, tag, promote, restrict, remove).
    fn inline_member_row(
        &self,
        chat_id: quill::ids::ChatId,
        row: &MemberRow,
        roots: &[std::path::PathBuf],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let session = self.session();
        let photo = row.user_id.and_then(|id| {
            session
                .and_then(|s| s.user_photo_path(id))
                .and_then(|path| sandboxed_display_path(path, roots))
        });
        let (status_text, online) = if row.is_bot {
            ("bot".to_string(), false)
        } else {
            (row.status.display(), row.status.is_online())
        };
        let badge = member_badge(row.member.status, &row.member.tag);
        let id = row.user_id.unwrap_or_default();
        let mut line = div()
            .id(format!("inline-member-{id}"))
            .flex()
            .items_center()
            .gap_3()
            .w_full()
            .px_2()
            .py_1p5()
            .rounded_md()
            .hover(|style| style.bg(cx.theme().secondary))
            .child(chat_avatar(&row.name, photo.as_deref(), 36.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_sm()
                            .truncate()
                            .child(super::bidi_line::one_line_plain(row.name.clone())),
                    )
                    .when(!status_text.is_empty(), |this| {
                        this.child(
                            div()
                                .text_xs()
                                .truncate()
                                .text_color(if online {
                                    cx.theme().primary
                                } else {
                                    cx.theme().muted_foreground
                                })
                                .child(status_text),
                        )
                    }),
            )
            .when_some(badge, |this, badge| {
                let color = cx.theme().primary;
                this.child(
                    div()
                        .flex_none()
                        .text_xs()
                        .when(badge.pill, |pill| {
                            pill.px_2().py_0p5().rounded_full().bg(color.opacity(0.14))
                        })
                        .text_color(if badge.pill {
                            color
                        } else {
                            cx.theme().muted_foreground
                        })
                        .child(super::bidi_line::one_line_plain(badge.text)),
                )
            });
        if let Some(user_id) = row.user_id {
            line = line
                .cursor_pointer()
                .role(gpui_kit::Role::Button)
                .aria_label(row.name.clone())
                .tab_index(0)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_info_panel_target(InfoPanelTarget::User(user_id), window, cx);
                }));
        }
        match self.member_menu_build(chat_id, &row.member, cx) {
            Some(build) => line
                .context_menu(move |menu, window, cx| build(menu, window, cx))
                .into_any_element(),
            None => line.into_any_element(),
        }
    }
}

/// The group's member count from `supergroupFullInfo`, when known.
fn target_member_count(app: &QuillApp, chat_id: quill::ids::ChatId) -> Option<i32> {
    let session = app.session()?;
    match session.chats.get(&chat_id.0)?.kind {
        ChatKind::Supergroup { supergroup_id, .. } => session
            .groups
            .supergroup_full_infos
            .get(&supergroup_id)
            .map(|info| info.member_count),
        _ => None,
    }
}
