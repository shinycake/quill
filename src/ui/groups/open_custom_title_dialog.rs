//! Methods moved out of `groups.rs` to keep files under 1000 lines.

use super::*;
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr, tr_with, trn_with};
use quill::state::{Submit, SubmitView};

impl QuillApp {
    /// "Leaving group…" or "Leaving channel…", by the chat's kind.
    pub(in crate::ui) fn leaving_line(&self, chat_id: ChatId) -> String {
        let channel = self
            .session()
            .and_then(|session| session.chats.get(&chat_id.0))
            .is_none_or(|chat| chat.is_channel());
        tr(if channel {
            keys::Q_PEOPLE_LEAVING_CHANNEL
        } else {
            keys::Q_PEOPLE_LEAVING_GROUP
        })
        .to_string()
    }

    /// Slice G1: admin custom-title prompt (`setChatMemberTag`, schema
    /// 1.8.67, line 13598 — the setter Telegram X's `EditRightsController`
    /// drives for "Custom title"). Basic groups and supergroups only.
    pub(in crate::ui) fn open_custom_title_dialog(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
        current: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.admin.username_dialog = Some(UsernameDialog::new(
            window,
            cx,
            TextPromptKind::CustomTitle { chat_id, user_id },
            current,
            quill::admin_extras::custom_title_placeholder(false),
        ));
        self.open_box(DialogKind::Username);
        cx.notify();
    }

    pub(in crate::ui) fn close_username_dialog(&mut self, cx: &mut Context<Self>) {
        self.admin.username_dialog = None;
        cx.notify();
    }

    pub(in crate::ui) fn submit_username_dialog(&mut self, cx: &mut Context<Self>) {
        let requests = self.session().map(|session| &session.requests);
        let Some(dialog) = self.admin.username_dialog.as_ref() else {
            return;
        };
        if dialog.submit.is_sending(requests) {
            return;
        }
        let value = dialog.input.read(cx).value().trim().to_string();
        let kind = dialog.kind;
        // The request, or the note that says why none was sent; the box
        // stays either way until TDLib answers.
        let sent: Result<RequestId, StatusNote> = match kind {
            TextPromptKind::CustomTitle { chat_id, user_id } => {
                // 0-16 characters, no emoji — schema line 13597, TGX
                // `EditRightsController` enforces the same client-side.
                let too_long = !quill::admin_extras::custom_title_fits(&value);
                let has_emoji = value.chars().any(looks_like_emoji);
                if too_long || has_emoji {
                    self.set_note(if too_long {
                        StatusNote::silent(tr(keys::Q_PEOPLE_CUSTOM_TITLE_MUST_BE_AT))
                    } else {
                        StatusNote::error(tr(keys::Q_PEOPLE_CUSTOM_TITLE_CANNOT_CONTAIN_EMOJI))
                    });
                    cx.notify();
                    return;
                }
                match self.live.as_mut() {
                    Some(live) => match live.driver.set_chat_member_tag(chat_id, user_id, &value) {
                        Ok(Some(id)) => Ok(id),
                        Ok(None) | Err(_) => Err(StatusNote::error(tr(
                            keys::Q_PEOPLE_COULD_NOT_SET_CUSTOM_TITLE,
                        ))),
                    },
                    None => Err(StatusNote::info(tr(
                        keys::Q_PEOPLE_CUSTOM_TITLES_NEED_A_LIVE,
                    ))),
                }
            }
            // Slice G10: `setCommunityName` (schema 1.8.67, line 11811;
            // the driver refuses empty names client-side). Not
            // optimistic — the new name arrives via `updateCommunity`.
            TextPromptKind::CommunityName { community_id } => {
                if value.is_empty() {
                    self.set_note(StatusNote::error(tr(
                        keys::Q_PEOPLE_COMMUNITY_NAME_CANNOT_BE_EMPTY,
                    )));
                    cx.notify();
                    return;
                }
                match self.live.as_mut() {
                    Some(live) => match live.driver.set_community_name(community_id, &value) {
                        Ok(Some(id)) => Ok(id),
                        Ok(None) | Err(_) => Err(StatusNote::error(tr(
                            keys::Q_PEOPLE_COULD_NOT_UPDATE_COMMUNITY_NAME,
                        ))),
                    },
                    None => Err(StatusNote::info(tr(
                        keys::Q_PEOPLE_RENAMING_COMMUNITIES_NEEDS_A_LIVE,
                    ))),
                }
            }
        };
        match sent {
            Ok(id) => {
                if let Some(dialog) = self.admin.username_dialog.as_mut() {
                    dialog.submit = Submit::sent(vec![id]);
                }
            }
            Err(note) => self.set_note(note),
        }
        cx.notify();
    }

    /// The custom title and community name box leaves its Sending stage on
    /// TDLib's answer: it closes with its note, or stays with the reason.
    pub(in crate::ui) fn settle_username_dialog(&mut self, cx: &mut Context<Self>) {
        let requests = self.session().map(|session| &session.requests);
        let Some(view) = self
            .admin
            .username_dialog
            .as_ref()
            .map(|dialog| dialog.submit.view(requests))
        else {
            return;
        };
        match view {
            SubmitView::Done => {
                let Some(dialog) = self.admin.username_dialog.take() else {
                    return;
                };
                let removed = dialog.input.read(cx).value().trim().is_empty();
                let key = match dialog.kind {
                    TextPromptKind::CustomTitle { .. } if removed => {
                        keys::Q_PEOPLE_CUSTOM_TITLE_REMOVED
                    }
                    TextPromptKind::CustomTitle { .. } => keys::Q_PEOPLE_CUSTOM_TITLE_UPDATED,
                    TextPromptKind::CommunityName { .. } => keys::Q_PEOPLE_COMMUNITY_NAME_UPDATED,
                };
                self.set_note(StatusNote::info(tr(key)));
                cx.notify();
            }
            SubmitView::Failed(reason) => {
                if let Some(dialog) = self.admin.username_dialog.as_mut()
                    && dialog.submit.keep_failure(reason)
                {
                    cx.notify();
                }
            }
            SubmitView::Editing | SubmitView::Sending => {}
        }
    }

    pub(in crate::ui) fn open_group_confirm(
        &mut self,
        action: GroupConfirmAction,
        cx: &mut Context<Self>,
    ) {
        self.admin.group_confirm_dialog = Some(GroupConfirmDialog { action });
        self.open_box(DialogKind::GroupConfirm);
        cx.notify();
    }

    /// Slice A6: display name for the user-scoped confirm dialogs
    /// (`DeleteContact`, `BlockContact`) — falls back to "User {id}"
    /// like the info panel does.
    pub(in crate::ui) fn contact_display_name(&self, user_id: i64) -> String {
        self.session()
            .and_then(|s| s.user(user_id))
            .map(|u| u.display_name())
            .unwrap_or_else(|| tr_with(keys::Q_PEOPLE_USER, &[("user_id", &user_id.to_string())]))
    }

    pub(in crate::ui) fn close_group_confirm(&mut self, cx: &mut Context<Self>) {
        self.admin.group_confirm_dialog = None;
        cx.notify();
    }

    /// Slice G1: run the confirmed action — `deleteChat` (schema
    /// 1.8.67, line 11850; driver checks
    /// `chat.can_be_deleted_for_all_users`), `leaveChat`, or the
    /// one-way `toggleSupergroupIsBroadcastGroup` upgrade (schema
    /// 1.8.67, line 15221).
    pub(in crate::ui) fn submit_group_confirm(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.admin.group_confirm_dialog.take() else {
            return;
        };
        // A driver's `Ok(None)` means the request was refused or is
        // already in flight — never report it as sent.
        fn sent_note(sent: Option<RequestId>, note: &str) -> String {
            if sent.is_some() {
                note.to_string()
            } else {
                tr(keys::Q_PEOPLE_REQUEST_ALREADY_IN_FLIGHT).to_string()
            }
        }
        // Slice B2: read before the live borrow — the RestartBot arm
        // needs the session while `live` is mutably borrowed.
        let restart_bot_user_id = match dialog.action {
            GroupConfirmAction::RestartBot { chat_id } => self
                .session()
                .and_then(|session| session.bot_user_id_for_chat(chat_id)),
            _ => None,
        };
        // The conversion asks twice (tdesktop): the intro leads to the warning.
        if let GroupConfirmAction::BroadcastIntro { chat_id } = dialog.action {
            // box-host: the intro leads to the warning in the same box
            self.admin.group_confirm_dialog = Some(GroupConfirmDialog {
                action: GroupConfirmAction::BroadcastUpgrade { chat_id },
            });
            cx.notify();
            return;
        }
        // A basic group must be left before its history goes.
        let is_basic_group = dialog
            .action
            .chat_id()
            .and_then(|chat_id| self.session()?.chats.get(&chat_id.0))
            .is_some_and(|chat| {
                matches!(
                    chat.kind,
                    quill::telegram::envelope::ChatKind::BasicGroup { .. }
                )
            });
        // Delete and Leave: the request the Edit box waits on, so the box
        // goes when the chat does and stays when TDLib refuses. Not while
        // the box's own Save is out: the exit would replace the stage that
        // reads that Save's answer.
        if matches!(
            dialog.action,
            GroupConfirmAction::DeleteChat { .. }
                | GroupConfirmAction::LeaveChat { .. }
                | GroupConfirmAction::RemoveFromList { .. }
        ) && dialog
            .action
            .chat_id()
            .is_some_and(|chat_id| self.group_edit_box_saving(chat_id))
        {
            // box-host: put back; the box stays where it shows
            self.admin.group_confirm_dialog = Some(dialog);
            self.set_note(StatusNote::info(tr(
                keys::Q_PEOPLE_REQUEST_ALREADY_IN_FLIGHT,
            )));
            cx.notify();
            return;
        }
        let mut exit_request = None;
        let note = match self.live.as_mut() {
            Some(live) => {
                let result = match dialog.action {
                    GroupConfirmAction::DeleteChat { chat_id } => {
                        live.driver.delete_chat(chat_id).map(|sent| {
                            exit_request = sent;
                            sent_note(sent, tr(keys::Q_PEOPLE_DELETING_CHAT))
                        })
                    }
                    GroupConfirmAction::LeaveChat { chat_id } => {
                        live.driver.leave_channel(chat_id).map(|id| {
                            exit_request = Some(id);
                            self.leaving_line(chat_id)
                        })
                    }
                    // Handled above; never reaches the driver.
                    GroupConfirmAction::BroadcastIntro { .. } => Ok("".to_string()),
                    GroupConfirmAction::BroadcastUpgrade { chat_id } => live
                        .driver
                        .upgrade_to_broadcast_group(chat_id)
                        // Ongoing, not done: TDLib answers `ok`/`error`
                        // asynchronously; the error arm rolls the
                        // optimistic flag back.
                        .map(|sent| {
                            sent_note(sent, tr(keys::Q_PEOPLE_CONVERTING_TO_BROADCAST_GROUP))
                        }),
                    // Slice B2: "Restart bot" — `deleteChatHistory` +
                    // `sendBotStartMessage` (schema lines 11845 / 12216).
                    // The bot user id is read from the session before the
                    // live borrow; a missing id fails closed with a status
                    // note, never silently.
                    GroupConfirmAction::RestartBot { chat_id } => match restart_bot_user_id {
                        Some(bot_user_id) => live
                            .driver
                            .restart_bot(chat_id, bot_user_id)
                            .map(|sent| sent_note(sent, tr(keys::Q_PEOPLE_RESTARTING_BOT))),
                        None => Ok(tr(keys::Q_PEOPLE_THIS_CHAT_IS_NO_LONGER).to_string()),
                    },
                    // Slice CL1: chat-list "Delete chat" —
                    // `deleteChatHistory` with `remove_from_chat_list:
                    // true` (Telegram X `Tdlib.deleteChat`).
                    GroupConfirmAction::RemoveFromList { chat_id } if is_basic_group => live
                        .driver
                        .delete_and_leave_basic_group(chat_id)
                        .map(|sent| {
                            exit_request = sent;
                            sent_note(sent, tr(keys::Q_PEOPLE_DELETING_CHAT))
                        }),
                    GroupConfirmAction::RemoveFromList { chat_id } => {
                        live.driver.remove_chat_from_list(chat_id).map(|sent| {
                            exit_request = sent;
                            sent_note(sent, tr(keys::Q_PEOPLE_DELETING_CHAT))
                        })
                    }
                    // Slice CL3: row-menu Block/Unblock
                    // (`setMessageSenderBlockList`, schema 1.8.67, line
                    // 14492). TDLib answers `ok`; the new state arrives
                    // via `updateChatBlockList`.
                    GroupConfirmAction::BlockUser { chat_id, block } => live
                        .driver
                        .set_chat_user_blocked(chat_id, block)
                        .map(|sent| {
                            sent_note(
                                sent,
                                if block {
                                    tr(keys::Q_PEOPLE_BLOCKING_USER)
                                } else {
                                    tr(keys::Q_PEOPLE_UNBLOCKING_USER)
                                },
                            )
                        }),
                    // Slice A6: user-panel Block/Unblock — the user-scoped
                    // twin (no chat to resolve through).
                    GroupConfirmAction::BlockContact { user_id, block } => {
                        live.driver.set_user_blocked(user_id, block).map(|sent| {
                            sent_note(
                                sent,
                                if block {
                                    tr(keys::Q_PEOPLE_BLOCKING_USER)
                                } else {
                                    tr(keys::Q_PEOPLE_UNBLOCKING_USER)
                                },
                            )
                        })
                    }
                    // Slice A6: user-panel "Delete contact" —
                    // `removeContacts` (schema 1.8.67, line 14528).
                    GroupConfirmAction::DeleteContact { user_id } => live
                        .driver
                        .remove_contact(user_id)
                        .map(|sent| sent_note(sent, tr(keys::Q_PEOPLE_DELETING_CONTACT))),
                    // Slice A6: "Delete synced contacts" —
                    // `clearImportedContacts` + `removeContacts`.
                    GroupConfirmAction::DeleteSyncedContacts => {
                        live.driver.delete_synced_contacts().map(|sent| {
                            if sent > 0 {
                                tr(keys::Q_PEOPLE_DELETING_SYNCED_CONTACTS).to_string()
                            } else {
                                tr(keys::Q_PEOPLE_NOTHING_TO_DELETE).to_string()
                            }
                        })
                    }
                    // Slice CL3: multi-select bulk delete — one
                    // `deleteChatHistory(remove_from_chat_list:true)`
                    // per chat that was selected when the box opened;
                    // the chats whose delete went out leave the selection; a
                    // refused one stays selected, to try again.
                    GroupConfirmAction::RemoveSelectedChats { ref ids } => {
                        let mut sent = 0;
                        let mut unsent = std::collections::HashSet::new();
                        for id in ids {
                            if live
                                .driver
                                .remove_chat_from_list(*id)
                                .is_ok_and(|sent| sent.is_some())
                            {
                                sent += 1;
                            } else {
                                unsent.insert(id.0);
                            }
                        }
                        let total = ids.len();
                        self.chat_list.selected.retain(|id| unsent.contains(id));
                        Ok(trn_with(
                            keys::Q_PEOPLE_DELETING_OF_CHATS,
                            total,
                            &[("sent", &sent.to_string())],
                        ))
                    }
                    GroupConfirmAction::RemoveSavedGif { file_id } => live
                        .driver
                        .set_gif_saved(file_id, false)
                        .map(|id| sent_note(id, tr(keys::Q_PEOPLE_REMOVING_SAVED_GIF))),
                    GroupConfirmAction::DeleteForumTopic {
                        chat_id,
                        forum_topic_id,
                    } => live
                        .driver
                        .delete_forum_topic(chat_id, forum_topic_id)
                        .map(|_| tr(keys::Q_PEOPLE_DELETING_TOPIC).to_string()),
                    GroupConfirmAction::DeleteSavedSublist { topic_id } => live
                        .driver
                        .delete_saved_topic_history(topic_id)
                        .map(|_| tr(keys::Q_PEOPLE_DELETING_SAVED_MESSAGES).to_string()),
                    GroupConfirmAction::RemoveEmojiSet { set_id } => live
                        .driver
                        .set_emoji_pack_installed(set_id, false)
                        .map(|id| sent_note(id, tr(keys::Q_PEOPLE_REMOVING_EMOJI_PACK))),
                    // Clearing stored payment information waits for TDLib confirmation.
                    GroupConfirmAction::ClearPaymentInfo => live
                        .driver
                        .clear_saved_payment_info()
                        .map(|_| tr(keys::Q_PEOPLE_CLEARING_SAVED_PAYMENT_INFO).to_string()),
                    // `deleteCommunity` (TDLib 1.8.68). The state drops the
                    // community on `ok`; a refusal surfaces through
                    // `Session::community_error`.
                    GroupConfirmAction::DeleteCommunity { community_id } => live
                        .driver
                        .delete_community(community_id)
                        .map(|sent| sent_note(sent, tr(keys::Q_PEOPLE_DELETING_COMMUNITY))),
                    GroupConfirmAction::RemoveMember { chat_id, user_id } => live
                        .driver
                        .remove_chat_member(chat_id, user_id)
                        .map(|id| sent_note(id, tr(keys::Q_PEOPLE_REMOVING_MEMBER))),
                };
                match result {
                    Ok(note) => {
                        // The chat goes: its Edit box goes with it, when
                        // TDLib has answered (`track_chat_exit`).
                        if let (Some(chat_id), Some(id)) = (dialog.action.chat_id(), exit_request) {
                            self.track_chat_exit(chat_id, id);
                        }
                        note
                    }
                    Err(_) => {
                        // box-host: put back after a failed submit; it stays where it shows
                        self.admin.group_confirm_dialog = Some(dialog);
                        tr(keys::Q_PEOPLE_ACTION_FAILED).to_string()
                    }
                }
            }
            None => {
                // box-host: put back after a failed submit; it stays where it shows
                self.admin.group_confirm_dialog = Some(dialog);
                tr(keys::Q_PEOPLE_CHAT_ACTIONS_NEED_A_LIVE).to_string()
            }
        };
        self.set_note(StatusNote::info(note));
        cx.notify();
    }

    /// kit Phase 2 (redo): username editor hosted in a kit `Dialog` via
    /// `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(in crate::ui) fn build_username_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::Username, |this, _, cx| {
                this.close_username_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true);
            let Some(dialog_state) = this.admin.username_dialog.as_ref() else {
                return dialog
                    .title(crate::ui::shell::dialog_title(tr(
                        keys::Q_PEOPLE_CUSTOM_TITLE,
                    )))
                    .on_close(on_close);
            };
            let (title, hint) = match dialog_state.kind {
                TextPromptKind::CustomTitle { .. } => (
                    tr(keys::Q_PEOPLE_CUSTOM_TITLE),
                    tr(keys::Q_PEOPLE_A_TITLE_THAT_MEMBERS_WILL),
                ),
                // Slice G10: community rename prompt.
                TextPromptKind::CommunityName { .. } => (
                    tr(keys::COMMUNITY_MANAGE_NAME),
                    tr(keys::Q_PEOPLE_SHOWN_IN_THE_COMMUNITIES_HUB),
                ),
            };
            // Admin titles show a live counter once fewer than half the
            // limit is left, like tdesktop's length-limited fields.
            let counter = matches!(dialog_state.kind, TextPromptKind::CustomTitle { .. })
                .then(|| {
                    let count = dialog_state.input.read(cx).value().trim().chars().count();
                    quill::admin_extras::length_counter(
                        count,
                        quill::admin_extras::CUSTOM_TITLE_LIMIT,
                    )
                })
                .flatten();
            let over_limit = counter
                .as_ref()
                .is_some_and(|text| text.starts_with('\u{2212}'));
            let body = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(hint),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div().flex_1().child(
                                Textarea::new(&dialog_state.input)
                                    .aria_label(tr(keys::Q_PEOPLE_PUBLIC_USERNAME_2))
                                    .h(px(40.)),
                            ),
                        )
                        .when_some(counter, |row, text| {
                            row.child(
                                div()
                                    .id("custom-title-counter")
                                    .text_sm()
                                    .text_color(if over_limit {
                                        cx.theme().danger
                                    } else {
                                        cx.theme().muted_foreground
                                    })
                                    .child(text),
                            )
                        }),
                )
                .when_some(
                    match &dialog_state.submit {
                        Submit::Failed(reason) => Some(reason.clone()),
                        _ => None,
                    },
                    |body, reason| {
                        body.child(
                            div()
                                .id("g1-username-refused")
                                .text_xs()
                                .text_color(cx.theme().danger)
                                .child(tr_with(
                                    keys::Q_PEOPLE_COULDNT_SAVE_REASON,
                                    &[("reason", &reason)],
                                )),
                        )
                    },
                )
                .into_any_element();
            let sending = dialog_state
                .submit
                .is_sending(this.session().map(|session| &session.requests));
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("g1-username-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_username_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::Username, window, cx);
                        })),
                )
                .child(
                    Button::new("g1-username-submit")
                        .label(tr(keys::SETTINGS_SAVE))
                        .loading(sending)
                        .disabled(sending)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_username_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::Username, window, cx);
                        })),
                );
            dialog
                .title(crate::ui::shell::dialog_title(title))
                .content(crate::ui::dialog_body::one_shot_body(
                    body.into_any_element(),
                ))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "group_confirm_tests.rs"]
mod tests;
