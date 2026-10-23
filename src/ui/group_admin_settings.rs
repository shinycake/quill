//! The pages the Edit box's manage rows open (tdesktop's boxes over
//! `EditPeerInfoBox`): reactions, the discussion group, invite links, join
//! requests, administrators, recent actions, boosts and the basic group
//! upgrade. Behaviour follows tdesktop's `edit_peer_info_box.cpp`,
//! `edit_peer_reactions.cpp` and `edit_discussion_link_box.cpp` (see
//! `docs/decisions/codex-group-admin-toggles.md` and
//! `docs/decisions/claude-edit-box-rows-counts.md`). Every change is gated
//! by `Session::group_admin_controls`, so a control shows only when the
//! viewer's rights allow it.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::dialogs::{GroupSettingsAction, GroupSettingsDialog, GroupSettingsView};
use super::pressable::action_row;
use super::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::dialog::DialogContent;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::group_manage::{custom_reactions_allowed, custom_reactions_over_level};
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with, trn_with};
use quill::state::{DiscussionControl, ProfileChatsFetch, ProfileChatsKind, Session};
use quill::telegram::envelope::{ChatAvailableReactions, ChatKind, ReactionType};
use std::cell::RefCell;
use std::rc::Rc;

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

    /// Open one page, refreshing what it reads.
    pub(super) fn open_group_settings_view(
        &mut self,
        chat_id: ChatId,
        view: GroupSettingsView,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.refresh_group_admin(chat_id);
        }
        self.admin.group_settings_dialog = Some(GroupSettingsDialog { chat_id, view });
        self.set_group_settings_view(view, cx);
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
        let channel = self.is_channel_chat(chat_id);
        if view == GroupSettingsView::RecentActions {
            self.start_event_log_poll(chat_id, cx);
            self.hook_event_log_scroll(cx);
        }
        if let Some(live) = self.live.as_mut() {
            let driver = &mut live.driver;
            let _ = match view {
                GroupSettingsView::Discussion if channel => {
                    driver.fetch_suitable_discussion_chats().map(|_| ())
                }
                GroupSettingsView::InviteLinks => driver
                    .fetch_chat_invite_links(chat_id)
                    .and_then(|_| driver.fetch_chat_invite_link_counts(chat_id))
                    .map(|_| ()),
                GroupSettingsView::JoinRequests => {
                    driver.fetch_chat_join_requests(chat_id).map(|_| ())
                }
                GroupSettingsView::Administrators => {
                    driver.fetch_chat_administrators(chat_id).map(|_| ())
                }
                GroupSettingsView::RecentActions => {
                    driver.fetch_chat_event_log(chat_id).map(|_| ())
                }
                _ => Ok(()),
            };
        }
        cx.notify();
    }

    /// Run one change on the open page's chat.
    pub(super) fn group_settings_action(
        &mut self,
        action: GroupSettingsAction,
        cx: &mut Context<Self>,
    ) {
        if let Some(chat_id) = self.admin.group_settings_dialog.as_ref().map(|d| d.chat_id) {
            self.group_chat_action(chat_id, action, cx);
        }
    }

    /// Run one change. Controls the viewer may not use are refused by the
    /// driver; the note says so instead of pretending.
    pub(super) fn group_chat_action(
        &mut self,
        chat_id: ChatId,
        action: GroupSettingsAction,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live.as_mut() else {
            self.connection
                .set_note(StatusNote::info(tr(keys::Q_PEOPLE_DEMO_MODE_THIS_NEEDS_A)));
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
            GroupSettingsAction::HiddenMembers(on) => driver.set_group_hidden_members(chat_id, on),
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
                self.connection.set_note(StatusNote::silent(tr(
                    keys::Q_PEOPLE_THAT_SETTING_ISNT_AVAILABLE_FOR,
                )));
            }
            Err(_) => {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_PEOPLE_COULDNT_SEND_THE_CHANGE_TRY,
                )));
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

    pub(super) fn settings_note(text: impl Into<SharedString>, muted: Hsla) -> Div {
        div()
            .px_2()
            .pb_2()
            .text_xs()
            .text_color(muted)
            .child(text.into())
    }

    /// A row with a trailing switch and a one-line explanation under it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn settings_switch(
        &self,
        chat_id: ChatId,
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
            .child(
                Switch::new((id, 1u64))
                    .checked(on)
                    .accessibility_label(label)
                    .small(),
            );
        if enabled {
            row = row.on_click(cx.listener(move |this, _, _, cx| {
                this.group_chat_action(chat_id, action.clone(), cx);
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
        if is_channel
            && let Some(custom) = custom_reactions_over_level(
                current.as_ref(),
                session
                    .groups
                    .chat_boost_status
                    .get(&chat_id.0)
                    .and_then(|(level, _)| {
                        custom_reactions_allowed(
                            session.groups.chat_boost_features.get(&chat_id.0),
                            *level,
                        )
                    }),
            )
        {
            let count = custom.to_string();
            body = body.child(Self::settings_note(
                trn_with(
                    keys::MANAGE_PEER_REACTIONS_LEVEL,
                    custom,
                    &[("same_count", &count)],
                )
                .replace("**", ""),
                muted,
            ));
        }
        if can_paid || paid_on {
            body = body.child(self.settings_switch(
                chat_id,
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
                                "{} {}",
                                tr_with(
                                    keys::MANAGE_DISCUSSION_GROUP_ABOUT_CHOSEN,
                                    &[("group", &title_of(group))]
                                ),
                                tr(keys::MANAGE_DISCUSSION_GROUP_POSTED)
                            ),
                            muted,
                        ))
                        .child(
                            action_row(
                                "b7-unlink",
                                Some(IconName::Ban),
                                tr(keys::MANAGE_DISCUSSION_GROUP_UNLINK),
                                true,
                                cx,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_group_settings_view(GroupSettingsView::ConfirmUnlink, cx);
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
                            "{} {}",
                            tr_with(
                                keys::MANAGE_LINKED_CHANNEL_ABOUT,
                                &[("channel", &title_of(channel))]
                            ),
                            tr(keys::MANAGE_LINKED_CHANNEL_POSTED)
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
        let close = |id: &'static str, label: &'static str, cx: &mut Context<Self>| {
            Button::new(id)
                .label(label)
                .ghost()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.close_group_settings_dialog(cx);
                    this.close_kit_dialog_if_done(DialogKind::GroupSettings, window, cx);
                }))
        };
        let done = div()
            .flex()
            .gap_2()
            .child(close("b7-close", tr(keys::BOX_DONE), cx))
            .into_any_element();
        let section =
            |section: Option<AnyElement>| section.unwrap_or_else(|| div().into_any_element());
        let title = self.chat_title_for_settings(chat_id);
        Some(match view {
            GroupSettingsView::Reactions => (
                tr(keys::MANAGE_PEER_REACTIONS).to_string(),
                self.settings_reactions(chat_id, cx),
                done,
            ),
            GroupSettingsView::Discussion => (
                if matches!(controls.discussion, Some(DiscussionControl::Group { .. })) {
                    tr(keys::Q_PEOPLE_LINKED_CHANNEL).to_string()
                } else {
                    tr(keys::MANAGE_DISCUSSION_GROUP).to_string()
                },
                self.settings_discussion(chat_id, cx),
                done,
            ),
            GroupSettingsView::Boosts => (
                tr(keys::Q_PEOPLE_BOOSTS).to_string(),
                self.settings_boosts(chat_id, cx),
                done,
            ),
            GroupSettingsView::InviteLinks => (
                tr(keys::MANAGE_PEER_INVITE_LINKS).to_string(),
                section(self.invite_links_section(chat_id, cx)),
                done,
            ),
            GroupSettingsView::JoinRequests => (
                tr(keys::MANAGE_PEER_REQUESTS).to_string(),
                section(self.join_requests_section(chat_id, cx)),
                done,
            ),
            GroupSettingsView::Administrators => (
                tr(keys::MANAGE_PEER_ADMINISTRATORS).to_string(),
                section(self.administrators_section(chat_id, cx)),
                done,
            ),
            GroupSettingsView::RecentActions => (
                tr(keys::MANAGE_PEER_RECENT_ACTIONS).to_string(),
                section(self.event_log_section(chat_id, cx)),
                done,
            ),
            GroupSettingsView::RecentActionsFilter => (
                tr(keys::MANAGE_PEER_RECENT_ACTIONS).to_string(),
                self.event_log_filter_body(cx),
                self.event_log_filter_footer(chat_id, cx),
            ),
            GroupSettingsView::ConfirmUpgrade => (
                tr(keys::Q_PEOPLE_UPGRADE_TO_SUPERGROUP).to_string(),
                div()
                    .text_sm()
                    .child(tr_with(
                        keys::Q_PEOPLE_UPGRADE_SUPERGROUP_SURE,
                        &[("title", &title)],
                    ))
                    .into_any_element(),
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("b7-upgrade-confirm")
                            .label(tr(keys::Q_PEOPLE_UPGRADE))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.group_settings_action(GroupSettingsAction::Upgrade, cx);
                                this.close_group_settings_dialog(cx);
                                this.close_kit_dialog_if_done(
                                    DialogKind::GroupSettings,
                                    window,
                                    cx,
                                );
                            })),
                    )
                    .child(close("b7-back", tr(keys::CANCEL), cx))
                    .into_any_element(),
            ),
            GroupSettingsView::ConfirmUnlink => {
                let text = match controls.discussion {
                    Some(DiscussionControl::Group { channel }) => {
                        let name = self.chat_title_for_settings(channel);
                        tr_with(
                            keys::Q_PEOPLE_UNLINK_FROM_THIS_GROUP_COMMENTS,
                            &[("name", &name)],
                        )
                    }
                    _ => {
                        let linked = self
                            .session()
                            .and_then(|s| s.chat_linked_chat(chat_id))
                            .map(|id| self.chat_title_for_settings(id))
                            .unwrap_or_else(|| tr(keys::Q_PEOPLE_THE_GROUP).into());
                        tr_with(
                            keys::Q_PEOPLE_UNLINK_FROM_POSTS_WILL_NO,
                            &[("linked", &linked), ("title", &title)],
                        )
                    }
                };
                let back = Button::new("b7-back")
                    .label(tr(keys::CREATE_GROUP_BACK))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.set_group_settings_view(GroupSettingsView::Discussion, cx);
                    }));
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
                                    this.set_group_settings_view(GroupSettingsView::Discussion, cx);
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
            // The recent actions keep their header in view and scroll the
            // events themselves, so that page is not wrapped in a scroll.
            let fixed = this
                .admin
                .group_settings_dialog
                .as_ref()
                .is_some_and(|dialog| dialog.view == GroupSettingsView::RecentActions);
            // `content` needs an `Fn` closure, but the body is built once
            // per dialog render — hand it over through a one-shot cell.
            let body = Rc::new(RefCell::new(Some(body)));
            let take = move || {
                body.borrow_mut()
                    .take()
                    .unwrap_or_else(|| div().into_any_element())
            };
            let content: Box<dyn Fn(DialogContent, &mut Window, &mut App) -> DialogContent> =
                if fixed {
                    Box::new(move |content, window, _| {
                        let height = (window.viewport_size().height - px(180.))
                            .max(px(240.))
                            .min(px(640.));
                        content
                            .min_h_0()
                            .child(div().w_full().h(height).child(take()))
                    })
                } else {
                    Box::new(crate::ui::shell::scrollable_dialog_content(
                        move |content, _, _| content.child(take()),
                    ))
                };
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(title))
                .content(content)
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
