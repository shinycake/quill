//! Methods moved out of `groups.rs` to keep files under 1000 lines.

use super::*;

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
                return dialog.title(crate::ui::shell::dialog_title("Confirm")).on_close(on_close);
            };
            let (title, message, confirm_label): (String, String, String) =
                match dialog_state.action {
                    GroupConfirmAction::DeleteChat => {
                        let is_channel = this
                            .session()
                            .and_then(|session| session.chats.get(&dialog_state.chat_id.0))
                            .is_some_and(|chat| chat.is_channel());
                        (
                            if is_channel { "Delete channel" } else { "Delete group" }
                                .to_string(),
                            quill::admin_extras::delete_chat_text(is_channel).to_string(),
                            "Delete".to_string(),
                        )
                    }
                    GroupConfirmAction::RemoveFromList => (
                        "Delete chat".to_string(),
                        "Delete this chat and its history from your chat list?".to_string(),
                        "Delete".to_string(),
                    ),
                    GroupConfirmAction::ReportChat => (
                        "Report chat".to_string(),
                        "Report this chat to Telegram moderators as spam?".to_string(),
                        "Report".to_string(),
                    ),
                    GroupConfirmAction::BlockUser { block } => {
                        if block {
                            (
                                "Block user".to_string(),
                                "Block this user? They won't be able to send you messages."
                                    .to_string(),
                                "Block".to_string(),
                            )
                        } else {
                            (
                                "Unblock user".to_string(),
                                "Unblock this user?".to_string(),
                                "Unblock".to_string(),
                            )
                        }
                    }
                    GroupConfirmAction::RemoveSelectedChats => (
                        "Delete chats".to_string(),
                        "Delete the selected chats and their history from your chat list?"
                            .to_string(),
                        "Delete".to_string(),
                    ),
                    GroupConfirmAction::LeaveChat => (
                        "Leave chat".to_string(),
                        "Leave this chat? You can rejoin with an invite link.".to_string(),
                        "Leave".to_string(),
                    ),
                    GroupConfirmAction::BroadcastIntro => (
                        "Broadcast Groups".to_string(),
                        quill::admin_extras::broadcast_features_text(),
                        "Convert".to_string(),
                    ),
                    GroupConfirmAction::BroadcastUpgrade => (
                        "Are you sure?".to_string(),
                        quill::admin_extras::BROADCAST_WARNING.to_string(),
                        "Convert".to_string(),
                    ),
                    GroupConfirmAction::ClearHistory { revoke } => (
                        "Clear history".to_string(),
                        if revoke {
                            "Delete all messages in this chat for everyone? This cannot be undone."
                        } else {
                            "Delete all messages in this chat for you? This cannot be undone."
                        }
                        .to_string(),
                        "Clear".to_string(),
                    ),
                    GroupConfirmAction::RestartBot => (
                        "Restart bot".to_string(),
                        "Clear this bot's chat history and send /start again? This cannot be undone."
                            .to_string(),
                        "Restart".to_string(),
                    ),
                    GroupConfirmAction::AbortRecoveryEmailSetup => (
                        "Abort recovery email setup".to_string(),
                        "Are you sure you want to abort recovery email setup? The new address will not be activated."
                            .to_string(),
                        "Abort".to_string(),
                    ),
                    GroupConfirmAction::BlockContact { user_id, block } => {
                        let name = this.contact_display_name(user_id);
                        if block {
                            (
                                "Block user".to_string(),
                                format!("Are you sure you want to block {name}?"),
                                "Block".to_string(),
                            )
                        } else {
                            (
                                "Unblock user".to_string(),
                                format!("Unblock {name}?"),
                                "Unblock".to_string(),
                            )
                        }
                    }
                    GroupConfirmAction::DeleteContact { user_id } => {
                        let name = this.contact_display_name(user_id);
                        (
                            "Delete contact".to_string(),
                            format!("Delete {name} from contacts?"),
                            "Delete".to_string(),
                        )
                    }
                    GroupConfirmAction::DeleteSyncedContacts => (
                        "Delete synced contacts".to_string(),
                        "This will remove your contacts from the Telegram servers. If 'Sync contacts' is enabled, contacts will be re-synced.".to_string(),
                        "Delete".to_string(),
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
                            .unwrap_or_else(|| "this topic".to_string());
                        (
                            "Delete topic".to_string(),
                            format!("Delete {name} and all its messages? This cannot be undone."),
                            "Delete".to_string(),
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
                            .unwrap_or_else(|| "this chat".to_string());
                        (
                            "Delete chat".to_string(),
                            format!(
                                "Delete all messages saved from {name}? This cannot be undone."
                            ),
                            "Delete".to_string(),
                        )
                    }
                    GroupConfirmAction::RemoveSavedGif { .. } => (
                        "Remove saved GIF".to_string(),
                        "Remove this GIF from your saved GIFs?".to_string(),
                        "Remove".to_string(),
                    ),
                    GroupConfirmAction::RemoveInstalledStickerSets => (
                        "Remove installed sticker sets".to_string(),
                        format!("Remove all {} installed sticker sets? You can install them again later.",this.session().map(|s|s.stickers.stickers.sets.len()).unwrap_or(0)),
                        "Remove all".to_string(),
                    ),
                    GroupConfirmAction::RemoveEmojiSet { .. } => ("Remove emoji pack".to_string(),"Remove this emoji pack? You can install it again later.".to_string(),"Remove".to_string()),
                    GroupConfirmAction::RemoveStickerSet { .. } => (
                        "Remove sticker set".to_string(),
                        "Remove this sticker set from your installed stickers? You can install it again later.".to_string(),
                        "Remove".to_string(),
                    ),
                    GroupConfirmAction::RemoveMember { user_id } => {
                        let name = this.contact_display_name(user_id);
                        let place = if this.group_flavor(dialog_state.chat_id) == Some(quill::moderation::GroupFlavor::Channel) {
                            "channel"
                        } else {
                            "group"
                        };
                        (
                            "Remove member".to_string(),
                            format!("Remove {name} from the {place}?"),
                            "Remove".to_string(),
                        )
                    }
                    GroupConfirmAction::ClearPaymentInfo => (
                        "Clear saved payment info".to_string(),
                        "Delete the shipping info and payment credentials Telegram saved from past checkouts? This cannot be undone.".to_string(),
                        "Clear".to_string(),
                    ),
                    GroupConfirmAction::DeleteCommunity { community_id } => {
                        let name = this
                            .session()
                            .and_then(|s| s.groups.communities.get(&community_id))
                            .map(|c| c.name.clone())
                            .unwrap_or_else(|| "this community".to_string());
                        (
                            "Delete community".to_string(),
                            format!(
                                "Delete {name} for all members? Its chats stay, but they leave the community. This cannot be undone."
                            ),
                            "Delete".to_string(),
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
                    | GroupConfirmAction::RemoveInstalledStickerSets
                    | GroupConfirmAction::RemoveStickerSet { .. }
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
                    .label("Cancel")
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
                    .tooltip("Direct messages")
                    .accessibility_label("Direct messages")
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
                            .child("Checking channel membership…"),
                    )
                    .into_any_element(),
            ),
            // Not subscribed: one clear action.
            Some(ChannelMemberStatus::Left) => Some(
                footer
                    .children(direct_button(cx))
                    .child(
                        Button::new("channel-join")
                            .label("Join channel")
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
                                    .label("Discuss")
                                    .ghost()
                                    .tooltip("Open the discussion group")
                                    .accessibility_label("Discuss")
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
                    format!("Posting as {}.", chat.title)
                } else {
                    "You are an admin, but posting is disabled for you.".to_string()
                };
                Some(
                    footer
                        .child(
                            Button::new("channel-leave")
                                .label("Leave channel")
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
                            .child(format!("Posting as {}.", chat.title)),
                    )
                    .into_any_element(),
            ),
            Some(ChannelMemberStatus::Banned) => Some(
                footer
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("You are banned from this channel."),
                    )
                    .into_any_element(),
            ),
            Some(ChannelMemberStatus::Restricted) | Some(ChannelMemberStatus::Unknown) => Some(
                footer
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Channel membership is unknown."),
                    )
                    .into_any_element(),
            ),
        }
    }

    /// `joinChat` for the open public channel. Demo sessions flip the status
    /// locally (no live driver).
    pub(in crate::ui) fn join_channel(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.join_channel(chat_id) {
                Ok(()) => "joining channel…".into(),
                Err(_) => "could not join channel".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(chat) = session.chats.get_mut(&chat_id.0) {
                chat.set_member_status(ChannelMemberStatus::Member, None);
            }
            self.connection.status_note = "joined channel (demo)".into();
        }
        cx.notify();
    }

    /// `leaveChat` for the open channel. Demo sessions flip the status locally.
    pub(in crate::ui) fn leave_channel(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.leave_channel(chat_id) {
                Ok(()) => "leaving channel…".into(),
                Err(_) => "could not leave channel".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some(chat) = session.chats.get_mut(&chat_id.0) {
                chat.set_member_status(ChannelMemberStatus::Left, None);
            }
            self.connection.status_note = "left channel (demo)".into();
        }
        cx.notify();
    }
}
