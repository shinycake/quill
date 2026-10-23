//! Methods moved out of `groups.rs` to keep files under 1000 lines.

use super::*;
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr, tr_with};

impl QuillApp {
    /// kit Phase 2 (redo): group confirm hosted in a kit `Dialog` via
    /// `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(in crate::ui) fn build_group_confirm_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::GroupConfirm, |this, _, cx| {
                this.close_group_confirm(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true);
            let Some(dialog_state) = this.admin.group_confirm_dialog.as_ref() else {
                return dialog.title(crate::ui::shell::dialog_title(tr(keys::SETTINGS_CLOUD_LOGIN_EMAIL_CONFIRM))).on_close(on_close);
            };
            let (title, message, confirm_label): (String, String, String) =
                match dialog_state.action {
                    GroupConfirmAction::DeleteChat => {
                        let is_channel = this
                            .session()
                            .and_then(|session| session.chats.get(&dialog_state.chat_id.0))
                            .is_some_and(|chat| chat.is_channel());
                        (
                            if is_channel { tr(keys::PROFILE_DELETE_CHANNEL) } else { tr(keys::PROFILE_DELETE_GROUP) }
                                .to_string(),
                            if is_channel { tr(keys::SURE_DELETE_CHANNEL) } else { tr(keys::SURE_DELETE_GROUP) }
                                .to_string(),
                            tr(keys::BOX_DELETE).to_string(),
                        )
                    }
                    GroupConfirmAction::RemoveFromList => {
                        let group = this
                            .session()
                            .and_then(|session| session.chats.get(&dialog_state.chat_id.0))
                            .filter(|chat| {
                                matches!(chat.kind, quill::telegram::envelope::ChatKind::BasicGroup { .. })
                            })
                            .map(|chat| chat.title.clone());
                        match group {
                            Some(group) => (
                                tr(keys::PROFILE_CLEAR_AND_EXIT).to_string(),
                                tr_with(keys::SURE_DELETE_AND_EXIT, &[("group", group.as_str())]),
                                tr(keys::GROUP_INVITE_CONTEXT_DELETE).to_string(),
                            ),
                            None => (
                                tr(keys::Q_PEOPLE_DELETE_CHAT).to_string(),
                                tr(keys::Q_PEOPLE_DELETE_THIS_CHAT_AND_ITS).to_string(),
                                tr(keys::GROUP_INVITE_CONTEXT_DELETE).to_string(),
                            ),
                        }
                    }
                    GroupConfirmAction::BlockUser { block } => {
                        if block {
                            (
                                tr(keys::PROFILE_BLOCK_USER).to_string(),
                                tr(keys::Q_PEOPLE_BLOCK_THIS_USER_THEY_WONT)
                                    .to_string(),
                                tr(keys::BLOCKED_LIST_CONFIRM_OK).to_string(),
                            )
                        } else {
                            (
                                tr(keys::PROFILE_UNBLOCK_USER).to_string(),
                                tr(keys::Q_PEOPLE_UNBLOCK_THIS_USER).to_string(),
                                tr(keys::Q_PEOPLE_UNBLOCK).to_string(),
                            )
                        }
                    }
                    GroupConfirmAction::RemoveSelectedChats => (
                        tr(keys::Q_PEOPLE_DELETE_CHATS).to_string(),
                        tr(keys::Q_PEOPLE_DELETE_THE_SELECTED_CHATS_AND)
                            .to_string(),
                        tr(keys::GROUP_INVITE_CONTEXT_DELETE).to_string(),
                    ),
                    GroupConfirmAction::LeaveChat => (
                        tr(keys::Q_PEOPLE_LEAVE_CHAT).to_string(),
                        tr(keys::Q_PEOPLE_LEAVE_THIS_CHAT_YOU_CAN).to_string(),
                        tr(keys::Q_PEOPLE_LEAVE).to_string(),
                    ),
                    GroupConfirmAction::BroadcastIntro => (
                        tr(keys::Q_PEOPLE_BROADCAST_GROUPS).to_string(),
                        quill::admin_extras::broadcast_features_text(),
                        tr(keys::Q_PEOPLE_CONVERT).to_string(),
                    ),
                    GroupConfirmAction::BroadcastUpgrade => (
                        tr(keys::Q_PEOPLE_ARE_YOU_SURE).to_string(),
                        quill::admin_extras::BROADCAST_WARNING.to_string(),
                        tr(keys::Q_PEOPLE_CONVERT).to_string(),
                    ),
                    GroupConfirmAction::RestartBot => (
                        tr(keys::Q_PEOPLE_RESTART_BOT).to_string(),
                        tr(keys::Q_PEOPLE_CLEAR_THIS_BOTS_CHAT_HISTORY)
                            .to_string(),
                        tr(keys::Q_PEOPLE_RESTART).to_string(),
                    ),
                    GroupConfirmAction::AbortRecoveryEmailSetup => (
                        tr(keys::Q_PEOPLE_ABORT_RECOVERY_EMAIL_SETUP).to_string(),
                        tr(keys::Q_PEOPLE_ARE_YOU_SURE_YOU_WANT)
                            .to_string(),
                        tr(keys::Q_PEOPLE_ABORT).to_string(),
                    ),
                    GroupConfirmAction::BlockContact { user_id, block } => {
                        let name = this.contact_display_name(user_id);
                        if block {
                            (
                                tr(keys::PROFILE_BLOCK_USER).to_string(),
                                tr_with(keys::Q_PEOPLE_ARE_YOU_SURE_YOU_WANT_2, &[("name", &name.to_string())]),
                                tr(keys::BLOCKED_LIST_CONFIRM_OK).to_string(),
                            )
                        } else {
                            (
                                tr(keys::PROFILE_UNBLOCK_USER).to_string(),
                                tr_with(keys::Q_PEOPLE_UNBLOCK_2, &[("name", &name.to_string())]),
                                tr(keys::Q_PEOPLE_UNBLOCK).to_string(),
                            )
                        }
                    }
                    GroupConfirmAction::DeleteContact { user_id } => {
                        let name = this.contact_display_name(user_id);
                        (
                            tr(keys::INFO_DELETE_CONTACT).to_string(),
                            tr_with(keys::Q_PEOPLE_DELETE_FROM_CONTACTS, &[("name", &name.to_string())]),
                            tr(keys::GROUP_INVITE_CONTEXT_DELETE).to_string(),
                        )
                    }
                    GroupConfirmAction::DeleteSyncedContacts => (
                        tr(keys::Q_PEOPLE_DELETE_SYNCED_CONTACTS).to_string(),
                        tr(keys::Q_PEOPLE_THIS_WILL_REMOVE_YOUR_CONTACTS).to_string(),
                        tr(keys::GROUP_INVITE_CONTEXT_DELETE).to_string(),
                    ),
                    GroupConfirmAction::DeleteForumTopic { forum_topic_id } => {
                        let name = this
                            .session()
                            .and_then(|s| {
                                s.threads.forum_topics.get(&dialog_state.chat_id.0).and_then(|topics| {
                                    topics
                                        .iter()
                                        .find(|t| t.forum_topic_id == forum_topic_id)
                                        .map(|t| t.name.clone())
                                })
                            })
                            .unwrap_or_else(|| tr(keys::Q_PEOPLE_THIS_TOPIC).to_string());
                        (
                            tr(keys::Q_PEOPLE_DELETE_TOPIC).to_string(),
                            tr_with(keys::Q_PEOPLE_DELETE_AND_ALL_ITS_MESSAGES, &[("name", &name.to_string())]),
                            tr(keys::GROUP_INVITE_CONTEXT_DELETE).to_string(),
                        )
                    }
                    GroupConfirmAction::DeleteSavedSublist { topic_id } => {
                        let name = this
                            .session()
                            .and_then(|s| {
                                s.threads.saved
                                    .topics
                                    .get(&topic_id)
                                    .map(|topic| s.saved_topic_title(topic))
                            })
                            .unwrap_or_else(|| tr(keys::Q_PEOPLE_THIS_CHAT).to_string());
                        (
                            tr(keys::Q_PEOPLE_DELETE_CHAT).to_string(),
                            format!(
                                "Delete all messages saved from {name}? This cannot be undone."
                            ),
                            tr(keys::GROUP_INVITE_CONTEXT_DELETE).to_string(),
                        )
                    }
                    GroupConfirmAction::RemoveSavedGif { .. } => (
                        tr(keys::Q_PEOPLE_REMOVE_SAVED_GIF).to_string(),
                        tr(keys::Q_PEOPLE_REMOVE_THIS_GIF_FROM_YOUR).to_string(),
                        tr(keys::BOX_REMOVE).to_string(),
                    ),
                    GroupConfirmAction::RemoveEmojiSet { .. } => (tr(keys::Q_PEOPLE_REMOVE_EMOJI_PACK).to_string(),tr(keys::Q_PEOPLE_REMOVE_THIS_EMOJI_PACK_YOU).to_string(),tr(keys::BOX_REMOVE).to_string()),
                    GroupConfirmAction::RemoveMember { user_id } => {
                        let name = this.contact_display_name(user_id);
                        let place = if this.group_flavor(dialog_state.chat_id) == Some(quill::moderation::GroupFlavor::Channel) {
                            "channel"
                        } else {
                            "group"
                        };
                        (
                            tr(keys::Q_PEOPLE_REMOVE_MEMBER).to_string(),
                            tr_with(keys::Q_PEOPLE_REMOVE_FROM_THE, &[("name", &name.to_string()), ("place", place)]),
                            tr(keys::BOX_REMOVE).to_string(),
                        )
                    }
                    GroupConfirmAction::ClearPaymentInfo => (
                        tr(keys::Q_PEOPLE_CLEAR_SAVED_PAYMENT_INFO).to_string(),
                        tr(keys::Q_PEOPLE_DELETE_THE_SHIPPING_INFO_AND).to_string(),
                        tr(keys::Q_PEOPLE_CLEAR).to_string(),
                    ),
                    GroupConfirmAction::DeleteCommunity { community_id } => {
                        let name = this
                            .session()
                            .and_then(|s| s.groups.communities.get(&community_id))
                            .map(|c| c.name.clone())
                            .unwrap_or_else(|| tr(keys::Q_PEOPLE_THIS_COMMUNITY).to_string());
                        (
                            tr(keys::Q_PEOPLE_DELETE_COMMUNITY).to_string(),
                            format!(
                                "Delete {name} for all members? Its chats stay, but they leave the community. This cannot be undone."
                            ),
                            tr(keys::GROUP_INVITE_CONTEXT_DELETE).to_string(),
                        )
                    }
                };
            let destructive = matches!(
                dialog_state.action,
                GroupConfirmAction::DeleteContact { .. }
                    | GroupConfirmAction::BlockUser { block: true, .. }
                    | GroupConfirmAction::BlockContact { block: true, .. }
                    | GroupConfirmAction::DeleteSyncedContacts
                    | GroupConfirmAction::ClearPaymentInfo
                    | GroupConfirmAction::RemoveSavedGif { .. }
                    | GroupConfirmAction::DeleteForumTopic { .. }
                    | GroupConfirmAction::DeleteSavedSublist { .. }
                    | GroupConfirmAction::RemoveEmojiSet { .. }
                    | GroupConfirmAction::DeleteCommunity { .. }
                    | GroupConfirmAction::RemoveMember { .. }
            );
            let body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_sm().child(message))
                .into_any_element();
            let footer = div().flex().justify_end().gap_2().child(
                Button::new("g1-confirm-cancel")
                    .label(tr(keys::CANCEL))
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_group_confirm(cx);
                        this.close_kit_dialog_if_done(DialogKind::GroupConfirm, window, cx);
                    })),
            ).child(
                Button::new("g1-confirm-submit")
                    .label(confirm_label)
                    .when(destructive, |button| button.danger())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.submit_group_confirm(cx);
                        this.close_kit_dialog_if_done(DialogKind::GroupConfirm, window, cx);
                    })),
            );
            dialog
                .title(crate::ui::shell::dialog_title(title))
                .content(crate::ui::shell::scrollable_dialog_content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body.borrow_mut().take().unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// Join/leave footer for an open broadcast channel (Phase 2.3). Admins
    /// with posting rights see the composer; the footer keeps the leave
    /// affordance and notes the posting state. Non-admins keep the 2.2
    /// behavior (composer hidden).
    pub(in crate::ui) fn channel_footer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        if !session.chats.get(&session.open_chat?.0)?.is_channel() {
            return None;
        }
        self.channel_footer_body(cx)
    }

    /// `channel_footer` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    /// The outer function repeats this body's first check; keep the two in step.
    #[inline(never)]
    fn channel_footer_body(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let open = session.open_chat?;
        let chat = session.chats.get(&open.0)?;
        if !chat.is_channel() {
            return None;
        }
        let status = chat.my_member_status;
        let muted = chat.is_muted();
        let footer = div()
            .p_3()
            .border_t_1()
            .border_color(cx.theme().border)
            .flex()
            .items_center()
            .justify_center()
            .gap_3();
        // tdesktop's direct-message button at the bar's left edge opens the
        // channel's direct messages group.
        let direct = session.chat_direct_messages_chat(open);
        let direct_button = |cx: &mut Context<Self>| {
            direct.map(|direct| {
                Button::new("channel-direct-messages")
                    .icon(gpui_kit::assets::IconName::MessageCircle)
                    .ghost()
                    .tooltip(tr(keys::MANAGE_MONOFORUM))
                    .accessibility_label(tr(keys::MANAGE_MONOFORUM))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select_listed_chat(direct, window, cx);
                    }))
            })
        };
        match status {
            None => Some(
                footer
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(keys::Q_PEOPLE_CHECKING_CHANNEL_MEMBERSHIP)),
                    )
                    .into_any_element(),
            ),
            // Not subscribed: one clear action.
            Some(ChannelMemberStatus::Left) => Some(
                footer
                    .children(direct_button(cx))
                    .child(
                        Button::new("channel-join")
                            .label(tr(keys::PROFILE_JOIN_CHANNEL))
                            .primary()
                            .w_full()
                            .max_w(px(360.))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.join_channel(open, cx);
                            })),
                    )
                    .into_any_element(),
            ),
            // Subscribers can't post: the bar mutes / unmutes the channel.
            // Leaving lives in the info panel (with confirmation).
            Some(ChannelMemberStatus::Member) => {
                // "Discuss" opens the linked discussion group, next to the
                // mute toggle.
                let discussion = session.discussion_chat_id(open);
                Some(
                    footer
                        .children(direct_button(cx))
                        .child(
                            Button::new("channel-mute-toggle")
                                .label(
                                    quill::chat_bottom_bar::BottomBar::MuteUnmute { muted }.label(),
                                )
                                .ghost()
                                .w_full()
                                .max_w(px(360.))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.apply_chat_mute(
                                        open,
                                        if muted {
                                            0
                                        } else {
                                            quill::telegram::envelope::MUTE_FOREVER
                                        },
                                        cx,
                                    );
                                })),
                        )
                        .when_some(discussion, |this, discussion| {
                            this.child(
                                Button::new("channel-discuss")
                                    .label(tr(keys::Q_PEOPLE_DISCUSS))
                                    .ghost()
                                    .tooltip(tr(keys::Q_PEOPLE_OPEN_THE_DISCUSSION_GROUP))
                                    .accessibility_label(tr(keys::Q_PEOPLE_DISCUSS))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.select_listed_chat(ChatId(discussion), window, cx);
                                    })),
                            )
                        })
                        .into_any_element(),
                )
            }
            Some(ChannelMemberStatus::Administrator) => {
                let can_post = chat.channel_admin_can_post();
                let note = if can_post {
                    tr_with(
                        keys::Q_PEOPLE_POSTING_AS,
                        &[("title", &chat.title.to_string())],
                    )
                } else {
                    tr(keys::Q_PEOPLE_YOU_ARE_AN_ADMIN_BUT).to_string()
                };
                Some(
                    footer
                        .child(
                            Button::new("channel-leave")
                                .label(tr(keys::PROFILE_LEAVE_CHANNEL))
                                .ghost()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.leave_channel(open, cx);
                                })),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(note),
                        )
                        .into_any_element(),
                )
            }
            Some(ChannelMemberStatus::Creator) => Some(
                footer
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr_with(
                                keys::Q_PEOPLE_POSTING_AS,
                                &[("title", &chat.title.to_string())],
                            )),
                    )
                    .into_any_element(),
            ),
            Some(ChannelMemberStatus::Banned) => Some(
                footer
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(keys::Q_PEOPLE_YOU_ARE_BANNED_FROM_THIS)),
                    )
                    .into_any_element(),
            ),
            Some(ChannelMemberStatus::Restricted) | Some(ChannelMemberStatus::Unknown) => Some(
                footer
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(keys::Q_PEOPLE_CHANNEL_MEMBERSHIP_IS_UNKNOWN)),
                    )
                    .into_any_element(),
            ),
        }
    }

    /// `joinChat` for the open public channel. Demo sessions flip the status
    /// locally (no live driver).
    pub(in crate::ui) fn join_channel(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection
                .set_note(match live.driver.join_channel(chat_id) {
                    Ok(()) => StatusNote::silent(tr(keys::Q_PEOPLE_JOINING_CHANNEL)),
                    Err(_) => StatusNote::error(tr(keys::Q_PEOPLE_COULD_NOT_JOIN_CHANNEL)),
                });
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(chat) = session.chats.get_mut(&chat_id.0) {
                chat.set_member_status(ChannelMemberStatus::Member, None);
            }
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_PEOPLE_JOINED_CHANNEL_DEMO)));
        }
        cx.notify();
    }

    /// `leaveChat` for the open channel. Demo sessions flip the status locally.
    pub(in crate::ui) fn leave_channel(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection
                .set_note(match live.driver.leave_channel(chat_id) {
                    Ok(()) => StatusNote::silent(tr(keys::Q_PEOPLE_LEAVING_CHANNEL)),
                    Err(_) => StatusNote::error(tr(keys::Q_PEOPLE_COULD_NOT_LEAVE_CHANNEL)),
                });
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(chat) = session.chats.get_mut(&chat_id.0) {
                chat.set_member_status(ChannelMemberStatus::Left, None);
            }
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_PEOPLE_LEFT_CHANNEL_DEMO)));
        }
        cx.notify();
    }
}
