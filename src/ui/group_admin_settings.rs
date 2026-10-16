//! B7: the group and channel settings dialog — topics, chat history for
//! new members, join to send, hidden members, content protection,
//! discussion group, allowed reactions and the basic group upgrade.
//! Behaviour follows tdesktop's `edit_peer_info_box.cpp`,
//! `edit_peer_type_box.cpp`, `edit_peer_reactions.cpp`,
//! `edit_members_visible.cpp` and `edit_discussion_link_box.cpp` (see
//! `docs/decisions/codex-group-admin-toggles.md`). Every row is gated by
//! `Session::group_admin_controls`, so a control shows only when the
//! viewer's rights allow it.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::dialogs::{GroupSettingsAction, GroupSettingsDialog, GroupSettingsView};
use super::pressable::action_row;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::state::{
    DiscussionControl, FORUM_MIN_MEMBERS, GroupAdminControls, ProfileChatsFetch, ProfileChatsKind,
    Session, TopicsLock,
};
use quill::telegram::envelope::{ChatAvailableReactions, ChatKind, ReactionType};
use std::cell::RefCell;
use std::rc::Rc;

/// The text in the Reactions row: "All", "Off" or the number allowed.
pub(super) fn reactions_summary(setting: Option<&ChatAvailableReactions>) -> String {
    match setting {
        None => "…".into(),
        Some(ChatAvailableReactions::All { .. }) => tr(keys::Q_PEOPLE_ALL).into(),
        Some(ChatAvailableReactions::Some { reactions, .. }) if reactions.is_empty() => {
            tr(keys::Q_PEOPLE_OFF).into()
        }
        Some(ChatAvailableReactions::Some { reactions, .. }) => reactions.len().to_string(),
    }
}

impl QuillApp {
    fn is_channel_chat(&self, chat_id: ChatId) -> bool {
        self.session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .is_some_and(|chat| {
                matches!(
                    chat.kind,
                    ChatKind::Supergroup {
                        is_channel: true,
                        ..
                    }
                )
            })
    }

    /// Open the settings dialog, refreshing the full info it reads.
    pub(super) fn open_group_settings_dialog(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.refresh_group_admin(chat_id);
        }
        self.admin.group_settings_dialog = Some(GroupSettingsDialog {
            chat_id,
            view: GroupSettingsView::Main,
        });
        cx.notify();
    }

    pub(super) fn close_group_settings_dialog(&mut self, cx: &mut Context<Self>) {
        self.admin.group_settings_dialog = None;
        cx.notify();
    }

    pub(super) fn set_group_settings_view(
        &mut self,
        view: GroupSettingsView,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self.admin.group_settings_dialog.as_mut() else {
            return;
        };
        dialog.view = view;
        let chat_id = dialog.chat_id;
        if view == GroupSettingsView::Boosts {
            self.open_boosts_page(chat_id, false);
        }
        if view == GroupSettingsView::Discussion
            && self.is_channel_chat(chat_id)
            && let Some(live) = self.live.as_mut()
        {
            let _ = live.driver.fetch_suitable_discussion_chats();
        }
        cx.notify();
    }

    /// Run one change. Controls the viewer may not use are refused by the
    /// driver; the note says so instead of pretending.
    pub(super) fn group_settings_action(
        &mut self,
        action: GroupSettingsAction,
        cx: &mut Context<Self>,
    ) {
        let Some(chat_id) = self.admin.group_settings_dialog.as_ref().map(|d| d.chat_id) else {
            return;
        };
        let Some(live) = self.live.as_mut() else {
            self.connection.status_note = tr(keys::Q_PEOPLE_DEMO_MODE_THIS_NEEDS_A).into();
            cx.notify();
            return;
        };
        let picker = live.driver.session.reaction_picker_emoji();
        let current = live
            .driver
            .session
            .chat_available_reactions(chat_id)
            .cloned();
        let driver = &mut live.driver;
        let result = match action {
            GroupSettingsAction::Topics(on) => driver.set_group_topics(chat_id, on, true),
            GroupSettingsAction::HistoryVisible(on) => {
                driver.set_group_history_visible(chat_id, on)
            }
            GroupSettingsAction::JoinToSend(on) => driver.set_group_join_to_send(chat_id, on),
            GroupSettingsAction::HiddenMembers(on) => driver.set_group_hidden_members(chat_id, on),
            GroupSettingsAction::ProtectedContent(on) => {
                driver.set_chat_protected_content(chat_id, on)
            }
            GroupSettingsAction::ReactionsAll => driver.set_chat_reactions(
                chat_id,
                ChatAvailableReactions::All {
                    max_reaction_count: current
                        .as_ref()
                        .map_or(11, ChatAvailableReactions::max_reaction_count),
                },
            ),
            GroupSettingsAction::ReactionsNone => {
                driver.set_chat_reactions(chat_id, ChatAvailableReactions::none())
            }
            // "Some" starts from the current list, or the default row.
            GroupSettingsAction::ReactionsSome => {
                let max = current
                    .as_ref()
                    .map_or(11, ChatAvailableReactions::max_reaction_count);
                let reactions = match &current {
                    Some(ChatAvailableReactions::Some { reactions, .. })
                        if !reactions.is_empty() =>
                    {
                        reactions.clone()
                    }
                    _ => picker.iter().take(6).map(ReactionType::emoji).collect(),
                };
                driver.set_chat_reactions(
                    chat_id,
                    ChatAvailableReactions::Some {
                        reactions,
                        max_reaction_count: max,
                    },
                )
            }
            GroupSettingsAction::ToggleEmoji(emoji) => {
                let next = Session::toggled_reaction(current.as_ref(), &picker, &emoji);
                driver.set_chat_reactions(chat_id, next)
            }
            GroupSettingsAction::PaidReaction(on) => {
                let max = current
                    .as_ref()
                    .map_or(11, ChatAvailableReactions::max_reaction_count);
                let mut reactions: Vec<ReactionType> = match &current {
                    Some(ChatAvailableReactions::Some { reactions, .. }) => reactions.clone(),
                    _ => picker.iter().map(ReactionType::emoji).collect(),
                };
                reactions.retain(|r| *r != ReactionType::Paid);
                if on {
                    reactions.push(ReactionType::Paid);
                }
                driver.set_chat_reactions(
                    chat_id,
                    ChatAvailableReactions::Some {
                        reactions,
                        max_reaction_count: max,
                    },
                )
            }
            GroupSettingsAction::LinkGroup(group) => driver.link_discussion_group(chat_id, group),
            GroupSettingsAction::Unlink => driver.unlink_discussion_group(chat_id),
            GroupSettingsAction::Upgrade => driver.upgrade_basic_group(chat_id),
        };
        match result {
            Ok(Some(_)) => {}
            Ok(None) => {
                self.connection.status_note =
                    tr(keys::Q_PEOPLE_THAT_SETTING_ISNT_AVAILABLE_FOR).into();
            }
            Err(_) => {
                self.connection.status_note = tr(keys::Q_PEOPLE_COULDNT_SEND_THE_CHANGE_TRY).into();
            }
        }
        cx.notify();
    }

    /// Poll-loop hook: after a basic group became a supergroup, close its
    /// settings and open the new chat. Returns `true` when something changed.
    pub(super) fn pump_chat_upgrades(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(live) = self.live.as_mut() else {
            return false;
        };
        let upgrades = live.driver.session.take_chat_upgrades();
        if upgrades.is_empty() {
            return false;
        }
        for (old, new) in upgrades {
            if self
                .admin
                .group_settings_dialog
                .as_ref()
                .is_some_and(|d| d.chat_id == old)
            {
                self.admin.group_settings_dialog = None;
            }
            if let Some(live) = self.live.as_mut()
                && live.driver.session.open_chat == Some(old)
            {
                let _ = live.driver.select_chat(new);
                self.dismiss_cross_chat_state(new, cx);
            }
        }
        cx.notify();
        true
    }

    fn settings_note(text: impl Into<SharedString>, muted: Hsla) -> Div {
        div()
            .px_2()
            .pb_2()
            .text_xs()
            .text_color(muted)
            .child(text.into())
    }

    /// A row with a trailing switch and a one-line explanation under it.
    #[allow(clippy::too_many_arguments)]
    fn settings_switch(
        &self,
        id: &'static str,
        icon: IconName,
        label: &'static str,
        on: bool,
        enabled: bool,
        about: String,
        action: GroupSettingsAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let mut row = action_row(id, Some(icon), label, false, cx)
            .aria_selected(on)
            .child(div().flex_1())
            .child(Switch::new((id, 1u64)).checked(on).small());
        if enabled {
            row = row.on_click(cx.listener(move |this, _, _, cx| {
                this.group_settings_action(action.clone(), cx);
            }));
        } else {
            row = row.opacity(0.5).cursor_default();
        }
        div()
            .flex()
            .flex_col()
            .child(row)
            .child(Self::settings_note(about, muted))
            .into_any_element()
    }

    /// A navigation row with the current value on the right.
    fn settings_link_row(
        &self,
        id: &'static str,
        icon: IconName,
        label: &'static str,
        value: String,
        view: GroupSettingsView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        action_row(id, Some(icon), label, false, cx)
            .child(div().flex_1())
            .child(div().text_sm().text_color(muted).child(value))
            .child(
                Icon::new(IconName::ChevronRight)
                    .size(px(14.))
                    .text_color(muted),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.set_group_settings_view(view, cx);
            }))
            .into_any_element()
    }

    fn settings_main(
        &self,
        chat_id: ChatId,
        controls: GroupAdminControls,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let session = self.session();
        let is_channel = self.is_channel_chat(chat_id);
        let muted = cx.theme().muted_foreground;
        let mut body = div().flex().flex_col().gap_1();
        let heading = |text: &'static str| {
            div()
                .px_2()
                .pt_2()
                .text_xs()
                .font_semibold()
                .text_color(muted)
                .child(text)
        };
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
            let can_toggle = !busy && (topics.enabled || topics.locked.is_none());
            body = body.child(self.settings_switch(
                "b7-topics",
                IconName::MessagesSquare,
                tr(keys::ADMIN_LOG_FILTER_TOPICS),
                topics.enabled,
                can_toggle,
                about,
                GroupSettingsAction::Topics(!topics.enabled),
                cx,
            ));
        }
        if let Some(history) = controls.history {
            let about = if history.visible {
                tr(keys::MANAGE_HISTORY_VISIBILITY_SHOWN_ABOUT)
            } else if history.needs_upgrade {
                tr(keys::Q_PEOPLE_NEW_MEMBERS_WONT_SEE_EARLIER)
            } else {
                tr(keys::MANAGE_HISTORY_VISIBILITY_HIDDEN_ABOUT)
            };
            body = body.child(self.settings_switch(
                "b7-history",
                IconName::Clock,
                tr(keys::MANAGE_HISTORY_VISIBILITY_TITLE),
                history.visible,
                !busy,
                about.into(),
                GroupSettingsAction::HistoryVisible(!history.visible),
                cx,
            ));
        }
        if controls.join_to_send {
            let on = session.is_some_and(|s| s.chat_join_to_send(chat_id));
            body =
                body.child(heading(tr(keys::MANAGE_PEER_SEND_TITLE)))
                    .child(self.settings_switch(
                        "b7-join-to-send",
                        IconName::UserCheck,
                        tr(keys::Q_PEOPLE_ONLY_MEMBERS),
                        on,
                        !busy,
                        tr(keys::MANAGE_PEER_SEND_ONLY_MEMBERS_ABOUT).into(),
                        GroupSettingsAction::JoinToSend(!on),
                        cx,
                    ));
            if self.chat_join_by_request(chat_id) {
                body = body.child(Self::settings_note(
                    tr(keys::MANAGE_PEER_SEND_APPROVE_MEMBERS_ABOUT),
                    muted,
                ));
            }
        }
        if controls.hide_members {
            let on = session
                .and_then(|s| {
                    s.chat_supergroup(chat_id)
                        .and_then(|id| s.supergroup_full_info(id))
                        .map(|info| info.admin.has_hidden_members)
                })
                .unwrap_or(false);
            body = body.child(self.settings_switch(
                "b7-hide-members",
                IconName::EyeOff,
                tr(keys::Q_PEOPLE_HIDE_MEMBERS),
                on,
                !busy,
                tr(keys::PROFILE_HIDE_PARTICIPANTS_ABOUT).into(),
                GroupSettingsAction::HiddenMembers(!on),
                cx,
            ));
        }
        if controls.protected_content {
            let on = session.is_some_and(|s| s.chat_has_protected_content(chat_id));
            body = body
                .child(heading(tr(keys::Q_PEOPLE_CONTENT_PROTECTION)))
                .child(
                    self.settings_switch(
                        "b7-protected",
                        IconName::Lock,
                        tr(keys::Q_PEOPLE_RESTRICT_SAVING_CONTENT),
                        on,
                        !busy,
                        if is_channel {
                            tr(keys::MANAGE_PEER_NO_FORWARDS_ABOUT_CHANNEL)
                        } else {
                            tr(keys::MANAGE_PEER_NO_FORWARDS_ABOUT)
                        }
                        .into(),
                        GroupSettingsAction::ProtectedContent(!on),
                        cx,
                    ),
                );
        }
        if let Some(discussion) = controls.discussion {
            let (label, value) = match discussion {
                DiscussionControl::Channel { linked } => (
                    tr(keys::MANAGE_DISCUSSION_GROUP),
                    linked
                        .and_then(|id| session.and_then(|s| s.chats.get(&id.0)))
                        .map_or_else(
                            || tr(keys::MANAGE_DISCUSSION_GROUP_ADD).to_string(),
                            |chat| chat.title.clone(),
                        ),
                ),
                DiscussionControl::Group { channel } => (
                    tr(keys::Q_PEOPLE_LINKED_CHANNEL),
                    session.and_then(|s| s.chats.get(&channel.0)).map_or_else(
                        || tr(keys::Q_PEOPLE_CHANNEL).to_string(),
                        |chat| chat.title.clone(),
                    ),
                ),
            };
            body = body.child(self.settings_link_row(
                "b7-discussion",
                IconName::Link,
                label,
                value,
                GroupSettingsView::Discussion,
                cx,
            ));
        }
        if controls.reactions {
            let summary =
                reactions_summary(session.and_then(|s| s.chat_available_reactions(chat_id)));
            body = body.child(self.settings_link_row(
                "b7-reactions",
                IconName::Heart,
                tr(keys::Q_PEOPLE_REACTIONS),
                summary,
                GroupSettingsView::Reactions,
                cx,
            ));
        }
        if controls.usernames {
            let count = session
                .and_then(|s| s.chat_usernames(chat_id))
                .map_or(0, |lists| lists.active.len());
            body = body.child(self.settings_link_row(
                "gl-usernames",
                IconName::Link,
                tr(keys::Q_PEOPLE_LINK_ORDER),
                format!("{count} active"),
                GroupSettingsView::Usernames,
                cx,
            ));
        }
        if controls.boosts {
            body = body.child(self.settings_link_row(
                "gl-boosts",
                IconName::Zap,
                tr(keys::Q_PEOPLE_BOOSTS),
                String::new(),
                GroupSettingsView::Boosts,
                cx,
            ));
        }
        if let Some(row) = self.chat_appearance_row(chat_id, cx) {
            body = body.child(row);
        }
        if let Some(row) = self.direct_messages_settings_row(chat_id, cx) {
            body = body.child(row);
        }
        if let Some(row) = self.channel_affiliate_row(chat_id, cx) {
            body = body.child(row);
        }
        if controls.upgrade {
            body = body.child(
                action_row(
                    "b7-upgrade",
                    Some(IconName::ArrowUp),
                    tr(keys::Q_PEOPLE_UPGRADE_TO_SUPERGROUP),
                    false,
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.set_group_settings_view(GroupSettingsView::ConfirmUpgrade, cx);
                })),
            );
        }
        if controls.is_empty() {
            body = body.child(
                div()
                    .px_2()
                    .text_sm()
                    .text_color(muted)
                    .child(tr(keys::Q_PEOPLE_YOU_DONT_HAVE_ANY_SETTINGS)),
            );
        }
        body.into_any_element()
    }

    fn settings_reactions(&self, chat_id: ChatId, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let Some(session) = self.session() else {
            return div().into_any_element();
        };
        let is_channel = self.is_channel_chat(chat_id);
        let current = session.chat_available_reactions(chat_id).cloned();
        let picker = session.reaction_picker_emoji();
        let busy = session.group_admin_busy(chat_id);
        let (all, none, some) = match &current {
            Some(ChatAvailableReactions::All { .. }) => (true, false, false),
            Some(setting @ ChatAvailableReactions::Some { .. }) => {
                (false, setting.is_none(), !setting.is_none())
            }
            None => (false, false, false),
        };
        let option = |id: &'static str,
                      label: &'static str,
                      about: &'static str,
                      selected: bool,
                      action: GroupSettingsAction,
                      cx: &mut Context<Self>| {
            div()
                .flex()
                .flex_col()
                .child({
                    let mut row = action_row(
                        id,
                        selected.then_some(IconName::CircleCheck),
                        label,
                        false,
                        cx,
                    )
                    .aria_selected(selected);
                    if busy {
                        row = row.opacity(0.5).cursor_default();
                    } else {
                        row = row.on_click(cx.listener(move |this, _, _, cx| {
                            this.group_settings_action(action.clone(), cx);
                        }));
                    }
                    row
                })
                .child(Self::settings_note(about, muted))
        };
        let (all_about, some_about, none_about) = if is_channel {
            (
                tr(keys::Q_PEOPLE_SUBSCRIBERS_CAN_USE_ANY_EMOJI),
                tr(keys::Q_PEOPLE_SUBSCRIBERS_CAN_USE_ONLY_CERTAIN),
                tr(keys::Q_PEOPLE_SUBSCRIBERS_CANT_ADD_ANY_REACTIONS),
            )
        } else {
            (
                tr(keys::MANAGE_PEER_REACTIONS_ALL_ABOUT),
                tr(keys::MANAGE_PEER_REACTIONS_SOME_ABOUT),
                tr(keys::MANAGE_PEER_REACTIONS_NONE_ABOUT),
            )
        };
        let mut body = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(option(
                "b7-reactions-all",
                tr(keys::Q_PEOPLE_ALL_REACTIONS),
                all_about,
                all,
                GroupSettingsAction::ReactionsAll,
                cx,
            ))
            .child(option(
                "b7-reactions-some",
                tr(keys::Q_PEOPLE_SOME_REACTIONS),
                some_about,
                some,
                GroupSettingsAction::ReactionsSome,
                cx,
            ))
            .child(option(
                "b7-reactions-none",
                tr(keys::Q_PEOPLE_NO_REACTIONS),
                none_about,
                none,
                GroupSettingsAction::ReactionsNone,
                cx,
            ));
        if let Some(ChatAvailableReactions::Some { reactions, .. }) = current.as_ref()
            && some
        {
            let mut grid = div().flex().flex_wrap().gap_1().px_2().pb_2();
            for (index, emoji) in picker.iter().enumerate() {
                let allowed = reactions
                    .iter()
                    .any(|r| r.emoji_text() == Some(emoji.as_str()));
                let value = emoji.clone();
                grid = grid.child(
                    Button::new(("b7-emoji", index as u64))
                        .label(emoji.clone())
                        .ghost()
                        .selected(allowed)
                        .disabled(busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.group_settings_action(
                                GroupSettingsAction::ToggleEmoji(value.clone()),
                                cx,
                            );
                        })),
                );
            }
            body = body
                .child(
                    div()
                        .px_2()
                        .pt_1()
                        .text_xs()
                        .font_semibold()
                        .text_color(muted)
                        .child(tr(keys::MANAGE_PEER_REACTIONS_SOME_TITLE)),
                )
                .child(grid);
        }
        let can_paid = is_channel
            && session
                .chat_supergroup(chat_id)
                .and_then(|id| session.supergroup_full_info(id))
                .is_some_and(|info| info.admin.can_enable_paid_reaction);
        let paid_on = matches!(
            &current,
            Some(ChatAvailableReactions::Some { reactions, .. }) if reactions.contains(&ReactionType::Paid)
        );
        if can_paid || paid_on {
            body = body.child(self.settings_switch(
                "b7-paid-reactions",
                IconName::Star,
                tr(keys::Q_PEOPLE_ENABLE_PAID_REACTIONS),
                paid_on,
                !busy,
                tr(keys::Q_PEOPLE_SWITCH_THIS_ON_TO_LET).into(),
                GroupSettingsAction::PaidReaction(!paid_on),
                cx,
            ));
        }
        body.into_any_element()
    }

    fn settings_discussion(&self, chat_id: ChatId, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let Some(session) = self.session() else {
            return div().into_any_element();
        };
        let busy = session.group_admin_busy(chat_id);
        let controls = session.group_admin_controls(chat_id);
        let title_of = |id: ChatId| {
            session.chats.get(&id.0).map_or_else(
                || tr(keys::Q_PEOPLE_THIS_CHAT).to_string(),
                |c| c.title.clone(),
            )
        };
        let mut body = div().flex().flex_col().gap_1();
        match controls.discussion {
            Some(DiscussionControl::Channel { linked }) => {
                if let Some(group) = linked {
                    body = body
                        .child(Self::settings_note(
                            format!(
                                "A link to {} is shown to all subscribers in the bottom panel. Everything you post in the channel is forwarded to this group.",
                                title_of(group)
                            ),
                            muted,
                        ))
                        .child(
                            action_row("b7-unlink", Some(IconName::Ban), tr(keys::MANAGE_DISCUSSION_GROUP_UNLINK), true, cx)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.set_group_settings_view(
                                        GroupSettingsView::ConfirmUnlink,
                                        cx,
                                    );
                                })),
                        );
                } else {
                    body = body.child(Self::settings_note(
                        tr(keys::MANAGE_DISCUSSION_GROUP_ABOUT),
                        muted,
                    ));
                    match session
                        .users_state
                        .profile_chat_lists
                        .get(&(ProfileChatsKind::SuitableDiscussionChats, 0))
                        .cloned()
                    {
                        None | Some(ProfileChatsFetch::Loading) => {
                            body = body.child(
                                div()
                                    .px_2()
                                    .text_sm()
                                    .text_color(muted)
                                    .child(tr(keys::Q_PEOPLE_LOADING)),
                            );
                        }
                        Some(ProfileChatsFetch::Failed(_)) => {
                            body = body.child(
                                action_row(
                                    "b7-discussion-retry",
                                    Some(IconName::RotateCw),
                                    tr(keys::Q_PEOPLE_COULDNT_LOAD_GROUPS_RETRY),
                                    false,
                                    cx,
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.retry_profile_chats(
                                            ProfileChatsKind::SuitableDiscussionChats,
                                            0,
                                            cx,
                                        );
                                    },
                                )),
                            );
                        }
                        Some(ProfileChatsFetch::Loaded(ids)) if ids.is_empty() => {
                            body = body.child(
                                div()
                                    .px_2()
                                    .text_sm()
                                    .text_color(muted)
                                    .child(tr(keys::Q_PEOPLE_YOU_DONT_HAVE_ANY_GROUPS)),
                            );
                        }
                        Some(ProfileChatsFetch::Loaded(ids)) => {
                            for id in ids {
                                let Some(title) = session.chats.get(&id).map(|c| c.title.clone())
                                else {
                                    continue;
                                };
                                let mut row = action_row(
                                    ("b7-discussion-choice", id.unsigned_abs()),
                                    Some(IconName::Users),
                                    title,
                                    false,
                                    cx,
                                );
                                if busy {
                                    row = row.opacity(0.5).cursor_default();
                                } else {
                                    row = row.on_click(cx.listener(move |this, _, _, cx| {
                                        this.group_settings_action(
                                            GroupSettingsAction::LinkGroup(ChatId(id)),
                                            cx,
                                        );
                                    }));
                                }
                                body = body.child(row);
                            }
                        }
                    }
                }
            }
            Some(DiscussionControl::Group { channel }) => {
                body = body
                    .child(Self::settings_note(
                        format!(
                            "This group is linked as the discussion board for {}. All new posts from the channel are forwarded to the group.",
                            title_of(channel)
                        ),
                        muted,
                    ))
                    .child(
                        action_row(
                            "b7-unlink",
                            Some(IconName::Ban),
                            tr(keys::Q_PEOPLE_UNLINK_CHANNEL),
                            true,
                            cx,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.set_group_settings_view(GroupSettingsView::ConfirmUnlink, cx);
                        })),
                    );
            }
            None => {
                body = body.child(Self::settings_note(
                    tr(keys::Q_PEOPLE_YOU_CANT_CHANGE_THE_DISCUSSION),
                    muted,
                ));
            }
        }
        body.into_any_element()
    }

    /// Title, body and footer of the dialog's current page.
    fn group_settings_parts(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<(String, AnyElement, AnyElement)> {
        let dialog = self.admin.group_settings_dialog.as_ref()?;
        let (chat_id, view) = (dialog.chat_id, dialog.view);
        let session = self.session()?;
        let controls = session.group_admin_controls(chat_id);
        let busy = session.group_admin_busy(chat_id);
        let channel = self.is_channel_chat(chat_id);
        let back = Button::new("b7-back")
            .label(tr(keys::CREATE_GROUP_BACK))
            .ghost()
            .on_click(cx.listener(|this, _, _, cx| {
                this.set_group_settings_view(GroupSettingsView::Main, cx);
            }));
        let close = Button::new("b7-close")
            .label(tr(keys::BOX_DONE))
            .on_click(cx.listener(|this, _, window, cx| {
                this.close_group_settings_dialog(cx);
                this.close_kit_dialog_if_done(DialogKind::GroupSettings, window, cx);
            }));
        let title = self.chat_title_for_settings(chat_id);
        Some(match view {
            GroupSettingsView::Main => (
                if channel {
                    tr(keys::ADMIN_LOG_FILTER_ACTIONS_CHANNEL_SETTINGS_SECTION).to_string()
                } else {
                    tr(keys::ADMIN_LOG_FILTER_ACTIONS_SETTINGS_SECTION).to_string()
                },
                self.settings_main(chat_id, controls, busy, cx),
                div().flex().gap_2().child(close).into_any_element(),
            ),
            GroupSettingsView::Reactions => (
                tr(keys::Q_PEOPLE_REACTIONS).to_string(),
                self.settings_reactions(chat_id, cx),
                div().flex().gap_2().child(back).into_any_element(),
            ),
            GroupSettingsView::Discussion => (
                if matches!(controls.discussion, Some(DiscussionControl::Group { .. })) {
                    tr(keys::Q_PEOPLE_LINKED_CHANNEL).to_string()
                } else {
                    tr(keys::MANAGE_DISCUSSION_GROUP).to_string()
                },
                self.settings_discussion(chat_id, cx),
                div().flex().gap_2().child(back).into_any_element(),
            ),
            GroupSettingsView::Usernames => (
                tr(keys::Q_PEOPLE_LINK_ORDER).to_string(),
                self.settings_usernames(chat_id, cx),
                div().flex().gap_2().child(back).into_any_element(),
            ),
            GroupSettingsView::Boosts => (
                tr(keys::Q_PEOPLE_BOOSTS).to_string(),
                self.settings_boosts(chat_id, cx),
                div().flex().gap_2().child(back).into_any_element(),
            ),
            GroupSettingsView::ConfirmUpgrade => (
                tr(keys::Q_PEOPLE_UPGRADE_TO_SUPERGROUP).to_string(),
                div()
                    .text_sm()
                    .child(format!(
                        "Upgrade \"{title}\" to a supergroup? The old group is deactivated and its members move to the new one. This can't be undone."
                    ))
                    .into_any_element(),
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("b7-upgrade-confirm")
                            .label(tr(keys::Q_PEOPLE_UPGRADE))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.group_settings_action(GroupSettingsAction::Upgrade, cx);
                                this.set_group_settings_view(GroupSettingsView::Main, cx);
                            })),
                    )
                    .child(back)
                    .into_any_element(),
            ),
            GroupSettingsView::ConfirmUnlink => {
                let text = match controls.discussion {
                    Some(DiscussionControl::Group { channel }) => {
                        let name = self.chat_title_for_settings(channel);
                        tr_with(keys::Q_PEOPLE_UNLINK_FROM_THIS_GROUP_COMMENTS, &[("name", &name.to_string())])
                    }
                    _ => {
                        let linked = self
                            .session()
                            .and_then(|s| s.chat_linked_chat(chat_id))
                            .map(|id| self.chat_title_for_settings(id))
                            .unwrap_or_else(|| tr(keys::Q_PEOPLE_THE_GROUP).into());
                        tr_with(keys::Q_PEOPLE_UNLINK_FROM_POSTS_WILL_NO, &[("linked", &linked.to_string()), ("title", &title.to_string())])
                    }
                };
                (
                    tr(keys::Q_PEOPLE_UNLINK).to_string(),
                    div()
                        .text_sm()
                        .text_color(cx.theme().foreground)
                        .child(text)
                        .into_any_element(),
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("b7-unlink-confirm")
                                .label(tr(keys::Q_PEOPLE_UNLINK))
                                .danger()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.group_settings_action(GroupSettingsAction::Unlink, cx);
                                    this.set_group_settings_view(GroupSettingsView::Main, cx);
                                })),
                        )
                        .child(back)
                        .into_any_element(),
                )
            }
        })
    }

    fn chat_title_for_settings(&self, chat_id: ChatId) -> String {
        self.session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .map_or_else(
                || tr(keys::Q_PEOPLE_THIS_CHAT).to_string(),
                |chat| chat.title.clone(),
            )
    }

    pub(super) fn build_group_settings_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::GroupSettings, |this, _, cx| {
                this.close_group_settings_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let Some((title, body, footer)) = this.group_settings_parts(cx) else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title(tr(keys::MENU_SETTINGS)))
                    .on_close(on_close);
            };
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(title))
                .content(crate::ui::shell::scrollable_dialog_content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body)));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// B7: group and channel settings (topics, history, reactions, ...).
    GroupSettings => DialogSpec::new(
        4900,
        |app| app.admin.group_settings_dialog.is_some(),
        QuillApp::build_group_settings_dialog,
    ),
}
