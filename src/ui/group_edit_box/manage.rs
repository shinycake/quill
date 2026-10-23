//! The Edit box's manage rows in tdesktop's order
//! (`Controller::fillManageSection`): type, discussion, direct messages,
//! history, topics, color, auto-translate, signatures, then the rows with
//! counts (reactions, permissions, invite links, administrators, members,
//! join requests, removed users, recent actions), affiliate programs,
//! Quill's own rows, stickers and Delete.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::dialogs::{GroupSettingsAction, GroupSettingsView, MemberTab};
use crate::ui::pressable::action_row;
use gpui_kit::assets::IconName as I;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use quill::group_manage::{ReactionsValue, positive_count};
use quill::group_type::type_row_value;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::state::{DiscussionControl, FORUM_MIN_MEMBERS, ManageRows, TopicsLock};
use quill::telegram::envelope::ChatKind;

impl QuillApp {
    /// The Edit box's manage rows under the name and description: each
    /// row the viewer's rights allow, then Delete.
    pub(super) fn group_edit_box_manage_rows(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let session = self.session();
        let kind = session
            .and_then(|s| s.chats.get(&chat_id.0))
            .map(|chat| &chat.kind);
        let is_channel = kind.is_some_and(|kind| kind.is_channel());
        let is_basic_group = matches!(kind, Some(ChatKind::BasicGroup { .. }));
        let is_owner = session.is_some_and(|s| s.chat_is_owner(chat_id));
        let controls = session
            .map(|s| s.group_admin_controls(chat_id))
            .unwrap_or_default();
        let rows = session.map(|s| s.manage_rows(chat_id)).unwrap_or_default();
        let mut section = div().flex().flex_col().w_full().gap_1();
        if is_owner {
            section = section.child(self.group_type_row(chat_id, is_channel, cx));
        }
        if let Some(discussion) = controls.discussion {
            section = section.child(self.discussion_row(chat_id, discussion, cx));
        }
        section = section.children(self.direct_messages_settings_row(chat_id, cx));
        section = section.children(self.group_history_row(chat_id, cx));
        if let Some(topics) = controls.topics {
            let about = match topics.locked {
                Some(TopicsLock::TooFewMembers) => tr_with(
                    keys::Q_PEOPLE_ONLY_GROUPS_WITH_MORE_THAN,
                    &[("forum_min_members", &FORUM_MIN_MEMBERS.to_string())],
                ),
                Some(TopicsLock::LinkedDiscussion) => tr(keys::FORUM_TOPICS_NO_DISCUSSION).into(),
                None if topics.needs_upgrade => tr(keys::Q_PEOPLE_THE_GROUP_CHAT_WILL_BE).into(),
                None => tr(keys::EDIT_TOPICS_ABOUT).into(),
            };
            let busy = session.is_some_and(|s| s.group_admin_busy(chat_id));
            section = section.child(self.settings_switch(
                chat_id,
                "b7-topics",
                I::MessagesSquare,
                tr(keys::ADMIN_LOG_FILTER_TOPICS),
                topics.enabled,
                !busy && (topics.enabled || topics.locked.is_none()),
                about,
                GroupSettingsAction::Topics(!topics.enabled),
                cx,
            ));
        }
        section = section.children(self.chat_appearance_row(chat_id, cx));
        section = section.children(self.auto_translate_row(chat_id, &rows, cx));
        section = section.children(self.signature_rows(&rows, cx));
        section = section.child(self.count_rows(chat_id, is_channel, &rows, cx));
        section = section.children(self.channel_affiliate_row(chat_id, cx));
        section = section.child(self.quill_manage_rows(chat_id, is_channel, is_basic_group, cx));
        section.into_any_element()
    }

    /// Reactions through Recent actions: the rows tdesktop draws with
    /// `AddButtonWithCount`.
    fn count_rows(
        &self,
        chat_id: ChatId,
        is_channel: bool,
        rows: &ManageRows,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let page = |view: GroupSettingsView| {
            move |this: &mut QuillApp,
                  _: &ClickEvent,
                  _: &mut Window,
                  cx: &mut Context<QuillApp>| {
                this.open_group_settings_view(chat_id, view, cx);
            }
        };
        let mut section = div().flex().flex_col().w_full().gap_1();
        if let Some(value) = rows.reactions {
            let value = value.map(|value| match value {
                ReactionsValue::All => tr(keys::MANAGE_PEER_REACTIONS_ON).to_string(),
                ReactionsValue::Off => tr(keys::MANAGE_PEER_REACTIONS_OFF).to_string(),
                ReactionsValue::Count(count) => count.to_string(),
            });
            section = section.child(value_row(
                "b7-reactions",
                I::Heart,
                tr(keys::MANAGE_PEER_REACTIONS),
                value,
                cx.listener(page(GroupSettingsView::Reactions)),
                cx,
            ));
        }
        section = section.children(self.welcome_row(chat_id, cx));
        if let Some((on, all)) = rows.permissions {
            section = section.child(value_row(
                "g1-open-permissions",
                I::Shield,
                tr(keys::MANAGE_PEER_PERMISSIONS),
                Some(format!("{on}/{all}")),
                cx.listener(move |this, _, _, cx| this.open_permissions_dialog(chat_id, cx)),
                cx,
            ));
        }
        if let Some(count) = rows.invite_links {
            section = section.child(value_row(
                "g1-invite-links",
                I::Link,
                tr(keys::MANAGE_PEER_INVITE_LINKS),
                positive_count(count),
                cx.listener(page(GroupSettingsView::InviteLinks)),
                cx,
            ));
        }
        if let Some(count) = rows.administrators {
            section = section.child(value_row(
                "g1-administrators",
                I::ShieldCheck,
                tr(keys::MANAGE_PEER_ADMINISTRATORS),
                positive_count(count),
                cx.listener(page(GroupSettingsView::Administrators)),
                cx,
            ));
        }
        if let Some(count) = rows.members {
            section = section.child(value_row(
                "g1-open-members",
                I::Users,
                if is_channel {
                    tr(keys::MANAGE_PEER_SUBSCRIBERS)
                } else {
                    tr(keys::MANAGE_PEER_MEMBERS)
                },
                positive_count(count),
                cx.listener(move |this, _, window, cx| {
                    this.open_member_dialog(chat_id, window, cx);
                }),
                cx,
            ));
        }
        if rows.join_requests > 0 {
            section = section.child(value_row(
                "g1-join-requests",
                I::UserPlus,
                if is_channel {
                    tr(keys::MANAGE_PEER_REQUESTS_CHANNEL)
                } else {
                    tr(keys::MANAGE_PEER_REQUESTS)
                },
                positive_count(rows.join_requests),
                cx.listener(page(GroupSettingsView::JoinRequests)),
                cx,
            ));
        }
        if let Some(count) = rows.removed_users {
            section = section.child(value_row(
                "g1-removed-users",
                I::UserX,
                tr(keys::MANAGE_PEER_REMOVED_USERS),
                positive_count(count),
                cx.listener(move |this, _, window, cx| {
                    this.open_member_dialog(chat_id, window, cx);
                    this.member_dialog_tab(MemberTab::Banned, cx);
                }),
                cx,
            ));
        }
        if rows.recent_actions {
            section = section.child(value_row(
                "g1-recent-actions",
                I::ListClock,
                tr(keys::MANAGE_PEER_RECENT_ACTIONS),
                None,
                cx.listener(move |this, _, window, cx| {
                    this.ensure_event_log_search(window, cx);
                    this.open_group_settings_view(chat_id, GroupSettingsView::RecentActions, cx);
                }),
                cx,
            ));
        }
        section.into_any_element()
    }

    /// "Discussion" with the linked group (or "Add a group") in a channel,
    /// "Linked channel" with its name in a discussion group.
    fn discussion_row(
        &self,
        chat_id: ChatId,
        discussion: DiscussionControl,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let session = self.session();
        let title = |id: ChatId| {
            session
                .and_then(|s| s.chats.get(&id.0))
                .map(|chat| chat.title.clone())
        };
        let (icon, label, value) = match discussion {
            DiscussionControl::Channel { linked } => (
                I::Users,
                tr(keys::MANAGE_DISCUSSION_GROUP),
                linked
                    .and_then(title)
                    .unwrap_or_else(|| tr(keys::MANAGE_DISCUSSION_GROUP_ADD).to_string()),
            ),
            DiscussionControl::Group { channel } => (
                I::Megaphone,
                tr(keys::Q_PEOPLE_LINKED_CHANNEL),
                title(channel).unwrap_or_default(),
            ),
        };
        value_row(
            "b7-discussion",
            icon,
            label,
            Some(value),
            cx.listener(move |this, _, _, cx| {
                this.open_group_settings_view(chat_id, GroupSettingsView::Discussion, cx);
            }),
            cx,
        )
    }

    /// "Welcome Message", "On" while one is set.
    fn welcome_row(&self, chat_id: ChatId, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        if !session.chat_can_send_welcome_messages(chat_id) {
            return None;
        }
        let on = session.chat_has_welcome_messages_flag(chat_id);
        Some(value_row(
            "g2-open-welcome",
            I::Hand,
            tr(keys::MANAGE_PEER_WELCOME_MESSAGES),
            on.then(|| tr(keys::Q_PEOPLE_ON).to_string()),
            cx.listener(move |this, _, window, cx| {
                this.open_welcome_dialog(chat_id, window, cx);
            }),
            cx,
        ))
    }

    /// The type row with its value ("Public", "Private", "Private
    /// restricted"), and the reason a saved link was refused.
    fn group_type_row(
        &self,
        chat_id: ChatId,
        is_channel: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let session = self.session();
        let edit = self.group_edit_box();
        let chosen = edit
            .and_then(|edit| edit.type_choice.clone())
            .or_else(|| session.map(|s| s.group_type_current(chat_id)))
            .unwrap_or_default();
        let value = tr(type_row_value(
            chosen.public,
            chosen.no_forwards,
            !is_channel,
        ));
        let failure = edit.and_then(|edit| edit.username_failure(session));
        let danger = cx.theme().danger;
        let label = if is_channel {
            tr(keys::MANAGE_PEER_CHANNEL_TYPE)
        } else {
            tr(keys::MANAGE_PEER_GROUP_TYPE)
        };
        div()
            .flex()
            .flex_col()
            .child(value_row(
                "group-edit-type",
                I::Globe,
                label,
                Some(value.to_string()),
                cx.listener(|this, _, window, cx| this.open_group_type_box(window, cx)),
                cx,
            ))
            .children(failure.map(|reason| {
                div().px_2().text_xs().text_color(danger).child(tr_with(
                    keys::Q_PEOPLE_COULDNT_SAVE_REASON,
                    &[("reason", &reason)],
                ))
            }))
            .into_any_element()
    }
}

/// A manage row with its value or count at the end (tdesktop
/// `AddButtonWithCount` / `AddButtonWithText`).
pub(super) fn value_row(
    id: &'static str,
    icon: I,
    label: impl Into<SharedString>,
    value: Option<String>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    action_row(id, Some(icon), label, false, cx)
        .child(div().flex_1())
        .when_some(value, |row, value| {
            row.child(div().text_sm().text_color(muted).child(value))
        })
        .on_click(on_click)
        .into_any_element()
}

/// One action of the manage section (`action_row` plus its handler).
pub(super) fn management_row(
    id: &'static str,
    icon: I,
    label: impl Into<SharedString>,
    destructive: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    action_row(id, Some(icon), label, destructive, cx)
        .on_click(on_click)
        .into_any_element()
}

/// An on/off setting of the manage section: the row toggles it; the
/// trailing switch shows the state (it carries no handler of its own, so
/// the click reaches the row). A locked switch shows a lock before it.
pub(super) fn management_toggle_row(
    id: &'static str,
    icon: I,
    label: impl Into<SharedString>,
    on: bool,
    locked: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let label = label.into();
    let muted = cx.theme().muted_foreground;
    action_row(id, Some(icon), label.clone(), false, cx)
        .aria_selected(on)
        .child(div().flex_1())
        .when(locked, |row| {
            row.child(Icon::new(I::Lock).size(px(14.)).text_color(muted))
        })
        .child(
            gpui_kit::component::switch::Switch::new((id, 1u64))
                .checked(on)
                .accessibility_label(label)
                .small(),
        )
        .on_click(on_click)
        .into_any_element()
}

/// Gray text under a group of rows (tdesktop `AddDividerText`).
pub(super) fn divider_text(text: impl Into<SharedString>, cx: &App) -> AnyElement {
    div()
        .px_2()
        .pb_2()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
        .into_any_element()
}
