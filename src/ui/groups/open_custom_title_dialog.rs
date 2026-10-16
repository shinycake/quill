//! Methods moved out of `groups.rs` to keep files under 1000 lines.

use super::*;
use quill::lang::{keys, tr, tr_with, trn, trn_with};

impl QuillApp {
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
            chat_id,
            TextPromptKind::CustomTitle { user_id },
            current,
            quill::admin_extras::custom_title_placeholder(false),
        ));
        cx.notify();
    }

    pub(in crate::ui) fn close_username_dialog(&mut self, cx: &mut Context<Self>) {
        self.admin.username_dialog = None;
        cx.notify();
    }

    pub(in crate::ui) fn submit_username_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.admin.username_dialog.take() else {
            return;
        };
        let value = dialog.input.read(cx).value().trim().to_string();
        let (kind, chat_id) = (dialog.kind, dialog.chat_id);
        let note = match kind {
            TextPromptKind::Username => match self.live.as_mut() {
                Some(live) => match live.driver.set_supergroup_username(chat_id, &value) {
                    Ok(_) => {
                        if value.is_empty() {
                            tr(keys::Q_PEOPLE_USERNAME_REMOVED).into()
                        } else {
                            tr_with(
                                keys::Q_PEOPLE_USERNAME_SET_TO,
                                &[("value", &value.to_string())],
                            )
                        }
                    }
                    Err(_) => {
                        self.admin.username_dialog = Some(dialog);
                        tr(keys::Q_PEOPLE_COULD_NOT_SET_USERNAME).into()
                    }
                },
                None => {
                    self.admin.username_dialog = Some(dialog);
                    tr(keys::Q_PEOPLE_USERNAMES_NEED_A_LIVE_CONNECTION).into()
                }
            },
            TextPromptKind::CustomTitle { user_id } => {
                // 0-16 characters, no emoji — schema line 13597, TGX
                // `EditRightsController` enforces the same client-side.
                let too_long = !quill::admin_extras::custom_title_fits(&value);
                let has_emoji = value.chars().any(looks_like_emoji);
                if too_long || has_emoji {
                    self.admin.username_dialog = Some(dialog);
                    self.connection.status_note = if too_long {
                        tr(keys::Q_PEOPLE_CUSTOM_TITLE_MUST_BE_AT).into()
                    } else {
                        tr(keys::Q_PEOPLE_CUSTOM_TITLE_CANNOT_CONTAIN_EMOJI).into()
                    };
                    cx.notify();
                    return;
                }
                match self.live.as_mut() {
                    Some(live) => match live.driver.set_chat_member_tag(chat_id, user_id, &value) {
                        Ok(_) => {
                            if value.is_empty() {
                                tr(keys::Q_PEOPLE_CUSTOM_TITLE_REMOVED).into()
                            } else {
                                tr(keys::Q_PEOPLE_CUSTOM_TITLE_UPDATED).into()
                            }
                        }
                        Err(_) => {
                            self.admin.username_dialog = Some(dialog);
                            tr(keys::Q_PEOPLE_COULD_NOT_SET_CUSTOM_TITLE).into()
                        }
                    },
                    None => {
                        self.admin.username_dialog = Some(dialog);
                        tr(keys::Q_PEOPLE_CUSTOM_TITLES_NEED_A_LIVE).into()
                    }
                }
            }
            TextPromptKind::GroupTitle => {
                // 1–128 chars per the schema (line 13430); the driver
                // re-validates before sending.
                if value.is_empty() || value.chars().count() > 128 {
                    self.admin.username_dialog = Some(dialog);
                    self.connection.status_note = tr(keys::Q_PEOPLE_TITLE_MUST_BE_1_128).into();
                    cx.notify();
                    return;
                }
                match self.live.as_mut() {
                    Some(live) => match live.driver.set_group_title(chat_id, &value) {
                        Ok(Some(_)) => tr(keys::Q_PEOPLE_TITLE_UPDATED).into(),
                        Ok(None) => {
                            self.admin.username_dialog = Some(dialog);
                            tr(keys::Q_PEOPLE_YOU_CANT_CHANGE_THIS_GROUPS).into()
                        }
                        Err(_) => {
                            self.admin.username_dialog = Some(dialog);
                            tr(keys::Q_PEOPLE_COULD_NOT_UPDATE_TITLE).into()
                        }
                    },
                    None => {
                        self.admin.username_dialog = Some(dialog);
                        tr(keys::Q_PEOPLE_TITLES_NEED_A_LIVE_CONNECTION).into()
                    }
                }
            }
            // Slice G10: `setCommunityName` (schema 1.8.67, line 11811;
            // the driver refuses empty names client-side). Not
            // optimistic — the new name arrives via `updateCommunity`.
            TextPromptKind::CommunityName { community_id } => {
                if value.is_empty() {
                    self.admin.username_dialog = Some(dialog);
                    self.connection.status_note =
                        tr(keys::Q_PEOPLE_COMMUNITY_NAME_CANNOT_BE_EMPTY).into();
                    cx.notify();
                    return;
                }
                match self.live.as_mut() {
                    Some(live) => match live.driver.set_community_name(community_id, &value) {
                        Ok(_) => tr(keys::Q_PEOPLE_COMMUNITY_NAME_UPDATED).into(),
                        Err(_) => {
                            self.admin.username_dialog = Some(dialog);
                            tr(keys::Q_PEOPLE_COULD_NOT_UPDATE_COMMUNITY_NAME).into()
                        }
                    },
                    None => {
                        self.admin.username_dialog = Some(dialog);
                        tr(keys::Q_PEOPLE_RENAMING_COMMUNITIES_NEEDS_A_LIVE).into()
                    }
                }
            }
            TextPromptKind::GroupDescription => {
                // 0–255 chars per the schema (line 13533); empty clears.
                if value.chars().count() > 255 {
                    self.admin.username_dialog = Some(dialog);
                    self.connection.status_note =
                        tr(keys::Q_PEOPLE_DESCRIPTION_MUST_BE_AT_MOST).into();
                    cx.notify();
                    return;
                }
                match self.live.as_mut() {
                    Some(live) => match live.driver.set_group_description(chat_id, &value) {
                        Ok(Some(_)) => {
                            if value.is_empty() {
                                tr(keys::Q_PEOPLE_DESCRIPTION_CLEARED).into()
                            } else {
                                tr(keys::Q_PEOPLE_DESCRIPTION_UPDATED).into()
                            }
                        }
                        Ok(None) => {
                            self.admin.username_dialog = Some(dialog);
                            tr(keys::Q_PEOPLE_YOU_CANT_CHANGE_THIS_GROUPS).into()
                        }
                        Err(_) => {
                            self.admin.username_dialog = Some(dialog);
                            tr(keys::Q_PEOPLE_COULD_NOT_UPDATE_DESCRIPTION).into()
                        }
                    },
                    None => {
                        self.admin.username_dialog = Some(dialog);
                        tr(keys::Q_PEOPLE_DESCRIPTIONS_NEED_A_LIVE_CONNECTION).into()
                    }
                }
            }
            TextPromptKind::GroupPhoto => {
                // Empty removes the photo; otherwise the path must be
                // a real file — TDLib would reject a missing one anyway.
                // `~` expands to the home dir.
                let value = quill::path_expand::expand_tilde(&value);
                let photo = if value.is_empty() {
                    None
                } else if std::path::Path::new(&value).is_file() {
                    Some(value.as_str())
                } else {
                    self.admin.username_dialog = Some(dialog);
                    self.connection.status_note = tr_with(
                        keys::Q_PEOPLE_FILE_NOT_FOUND,
                        &[("value", &value.to_string())],
                    );
                    cx.notify();
                    return;
                };
                match self.live.as_mut() {
                    Some(live) => match live.driver.set_group_photo(chat_id, photo) {
                        Ok(Some(_)) => {
                            if value.is_empty() {
                                tr(keys::Q_PEOPLE_PHOTO_REMOVED).into()
                            } else {
                                tr(keys::Q_PEOPLE_PHOTO_UPDATED).into()
                            }
                        }
                        Ok(None) => {
                            self.admin.username_dialog = Some(dialog);
                            tr(keys::Q_PEOPLE_YOU_CANT_CHANGE_THIS_GROUPS).into()
                        }
                        Err(_) => {
                            self.admin.username_dialog = Some(dialog);
                            tr(keys::Q_PEOPLE_COULD_NOT_UPDATE_PHOTO).into()
                        }
                    },
                    None => {
                        self.admin.username_dialog = Some(dialog);
                        tr(keys::Q_PEOPLE_PHOTOS_NEED_A_LIVE_CONNECTION).into()
                    }
                }
            }
        };
        self.connection.status_note = note;
        cx.notify();
    }

    pub(in crate::ui) fn open_group_confirm(
        &mut self,
        chat_id: ChatId,
        action: GroupConfirmAction,
        cx: &mut Context<Self>,
    ) {
        self.admin.group_confirm_dialog = Some(GroupConfirmDialog { chat_id, action });
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
        let restart_bot_user_id = if matches!(dialog.action, GroupConfirmAction::RestartBot) {
            self.session()
                .and_then(|session| session.bot_user_id_for_chat(dialog.chat_id))
        } else {
            None
        };
        // The conversion asks twice (tdesktop): the intro leads to the warning.
        if matches!(dialog.action, GroupConfirmAction::BroadcastIntro) {
            self.admin.group_confirm_dialog = Some(GroupConfirmDialog {
                chat_id: dialog.chat_id,
                action: GroupConfirmAction::BroadcastUpgrade,
            });
            cx.notify();
            return;
        }
        // The chats still in the list, read before the live borrow.
        let selected_chats = self.selected_chat_ids();
        let note = match self.live.as_mut() {
            Some(live) => {
                let result = match dialog.action {
                    GroupConfirmAction::DeleteChat => live
                        .driver
                        .delete_chat(dialog.chat_id)
                        .map(|sent| sent_note(sent, tr(keys::Q_PEOPLE_CHAT_DELETED))),
                    GroupConfirmAction::LeaveChat => live
                        .driver
                        .leave_channel(dialog.chat_id)
                        .map(|_| tr(keys::Q_PEOPLE_LEFT_THE_CHAT).to_string()),
                    // Handled above; never reaches the driver.
                    GroupConfirmAction::BroadcastIntro => Ok("".to_string()),
                    GroupConfirmAction::BroadcastUpgrade => live
                        .driver
                        .upgrade_to_broadcast_group(dialog.chat_id)
                        // Ongoing, not done: TDLib answers `ok`/`error`
                        // asynchronously; the error arm rolls the
                        // optimistic flag back.
                        .map(|sent| {
                            sent_note(sent, tr(keys::Q_PEOPLE_CONVERTING_TO_BROADCAST_GROUP))
                        }),
                    // Slice CL1: `deleteChatHistory` (schema 1.8.67,
                    // line 11845). TDLib answers `ok`/`error`
                    // asynchronously; a refusal surfaces via
                    // `Session::chat_action_error`.
                    GroupConfirmAction::ClearHistory { revoke } => live
                        .driver
                        .clear_chat_history(dialog.chat_id, revoke)
                        .map(|sent| sent_note(sent, tr(keys::Q_PEOPLE_CLEARING_HISTORY))),
                    // Slice B2: "Restart bot" — `deleteChatHistory` +
                    // `sendBotStartMessage` (schema lines 11845 / 12216).
                    // The bot user id is read from the session before the
                    // live borrow; a missing id fails closed with a status
                    // note, never silently.
                    GroupConfirmAction::RestartBot => match restart_bot_user_id {
                        Some(bot_user_id) => live
                            .driver
                            .restart_bot(dialog.chat_id, bot_user_id)
                            .map(|sent| sent_note(sent, tr(keys::Q_PEOPLE_RESTARTING_BOT))),
                        None => Ok(tr(keys::Q_PEOPLE_THIS_CHAT_IS_NO_LONGER).to_string()),
                    },
                    // Slice CL1: chat-list "Delete chat" —
                    // `deleteChatHistory` with `remove_from_chat_list:
                    // true` (Telegram X `Tdlib.deleteChat`).
                    GroupConfirmAction::RemoveFromList => live
                        .driver
                        .remove_chat_from_list(dialog.chat_id)
                        .map(|sent| sent_note(sent, tr(keys::Q_PEOPLE_DELETING_CHAT))),
                    // Slice CL3: row-menu Block/Unblock
                    // (`setMessageSenderBlockList`, schema 1.8.67, line
                    // 14492). TDLib answers `ok`; the new state arrives
                    // via `updateChatBlockList`.
                    GroupConfirmAction::BlockUser { block } => live
                        .driver
                        .set_chat_user_blocked(dialog.chat_id, block)
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
                    // per selected chat; the selection clears on
                    // confirm. The ids come from the live selection
                    // (the dialog blocks selection changes while
                    // open), so the enum stays `Copy`.
                    GroupConfirmAction::RemoveSelectedChats => {
                        let chat_ids: Vec<ChatId> = selected_chats;
                        let mut sent = 0;
                        for id in &chat_ids {
                            if live
                                .driver
                                .remove_chat_from_list(*id)
                                .is_ok_and(|sent| sent.is_some())
                            {
                                sent += 1;
                            }
                        }
                        let total = chat_ids.len();
                        self.chat_list.selected.clear();
                        Ok(trn_with(
                            keys::Q_PEOPLE_DELETING_OF_CHATS,
                            total,
                            &[("sent", &sent.to_string())],
                        ))
                    }
                    // Slice A2: abort the pending recovery-email setup
                    // (`cancelRecoveryEmailAddressVerification`, schema
                    // 1.8.67, line 11467). The dialog carries no chat, so
                    // `dialog.chat_id` is unused here.
                    GroupConfirmAction::AbortRecoveryEmailSetup => live
                        .driver
                        .cancel_recovery_email_setup()
                        .map(|_| tr(keys::Q_PEOPLE_ABORTING_EMAIL_SETUP).to_string()),
                    GroupConfirmAction::RemoveSavedGif { file_id } => live
                        .driver
                        .set_gif_saved(file_id, false)
                        .map(|id| sent_note(id, tr(keys::Q_PEOPLE_REMOVING_SAVED_GIF))),
                    GroupConfirmAction::DeleteForumTopic { forum_topic_id } => live
                        .driver
                        .delete_forum_topic(dialog.chat_id, forum_topic_id)
                        .map(|_| tr(keys::Q_PEOPLE_DELETING_TOPIC).to_string()),
                    GroupConfirmAction::DeleteSavedSublist { topic_id } => live
                        .driver
                        .delete_saved_topic_history(topic_id)
                        .map(|_| tr(keys::Q_PEOPLE_DELETING_SAVED_MESSAGES).to_string()),
                    GroupConfirmAction::RemoveInstalledStickerSets => {
                        let ids: Vec<_> = live
                            .driver
                            .session
                            .stickers
                            .stickers
                            .sets
                            .iter()
                            .map(|set| set.id)
                            .collect();
                        live.driver.manage_sticker_sets(&ids, false).map(|sent| {
                            tr_with(
                                keys::Q_PEOPLE_REMOVING_STICKER_SETS,
                                &[("sent", &sent.to_string())],
                            )
                        })
                    }
                    GroupConfirmAction::RemoveEmojiSet { set_id } => live
                        .driver
                        .set_emoji_pack_installed(set_id, false)
                        .map(|id| sent_note(id, tr(keys::Q_PEOPLE_REMOVING_EMOJI_PACK))),
                    GroupConfirmAction::RemoveStickerSet { set_id } => live
                        .driver
                        .manage_sticker_set(set_id, false, false)
                        .map(|id| sent_note(id, tr(keys::Q_PEOPLE_REMOVING_STICKER_SET))),
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
                    GroupConfirmAction::RemoveMember { user_id } => live
                        .driver
                        .remove_chat_member(dialog.chat_id, user_id)
                        .map(|id| sent_note(id, tr(keys::Q_PEOPLE_REMOVING_MEMBER))),
                };
                match result {
                    Ok(note) => note,
                    Err(_) => {
                        self.admin.group_confirm_dialog = Some(dialog);
                        tr(keys::Q_PEOPLE_ACTION_FAILED).to_string()
                    }
                }
            }
            None => {
                self.admin.group_confirm_dialog = Some(dialog);
                tr(keys::Q_PEOPLE_CHAT_ACTIONS_NEED_A_LIVE).to_string()
            }
        };
        self.connection.status_note = note;
        cx.notify();
    }

    /// kit Phase 2 (redo): create-chat hosted in a kit `Dialog` via
    /// `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(in crate::ui) fn build_create_chat_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::CreateChat, |this, _, cx| {
                this.close_create_chat_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog.overlay(true);
            let Some(dialog_state) = this.admin.create_chat_dialog.as_ref() else {
                return dialog
                    .title(crate::ui::shell::dialog_title(tr(keys::Q_PEOPLE_NEW_CHAT)))
                    .on_close(on_close);
            };
            let kind = dialog_state.kind;
            let picks_members = kind.picks_members();
            let query = dialog_state.search_input.read(cx).value();
            let rows = if picks_members {
                this.g1_contact_rows(&query, cx)
            } else {
                Vec::new()
            };
            let selected = dialog_state.selected_users.clone();
            let mut body = div().flex().flex_col().gap_2();
            body = body
                .child(
                    div().flex_1().child(
                        Textarea::new(&dialog_state.title_input)
                            .aria_label(tr(keys::Q_PEOPLE_GROUP_OR_CHANNEL_TITLE))
                            .h(px(40.)),
                    ),
                )
                .when(kind != CreateChatKind::BasicGroup, |this| {
                    this.child(
                        div().flex_1().child(
                            Textarea::new(&dialog_state.description_input)
                                .aria_label(tr(keys::Q_PEOPLE_GROUP_OR_CHANNEL_DESCRIPTION))
                                .h(px(64.)),
                        ),
                    )
                });
            if picks_members {
                body = body.child(
                    div().flex().items_center().gap_2().child(
                        div().flex_1().child(
                            Textarea::new(&dialog_state.search_input)
                                .aria_label(tr(keys::Q_PEOPLE_SEARCH_PEOPLE_TO_INVITE))
                                .h(px(40.)),
                        ),
                    ),
                );
                let mut list = div()
                    .id("g1-create-contacts")
                    .flex()
                    .flex_col()
                    .gap_1()
                    .max_h(px(220.))
                    .overflow_y_scroll();
                if rows.is_empty() {
                    list = list.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(keys::CONTACTS_NOT_FOUND)),
                    );
                }
                for row in rows.iter().take(50) {
                    let is_selected = selected.contains(&row.user_id);
                    list = list.child(this.g1_contact_checkbox(
                        "g1-create".to_string(),
                        row,
                        is_selected,
                        row.user_id,
                        cx,
                    ));
                }
                body = body.child(list);
                if !selected.is_empty() {
                    body = body.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(trn(keys::Q_PEOPLE_MEMBERS_SELECTED, selected.len())),
                    );
                }
            }
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("g1-create-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_create_chat_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::CreateChat, window, cx);
                        })),
                )
                .child(
                    Button::new("g1-create-submit")
                        .label(tr_with(
                            keys::Q_PEOPLE_CREATE,
                            &[("to_lowercase", &(kind.title().to_lowercase()).to_string())],
                        ))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_create_chat_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::CreateChat, window, cx);
                        })),
                );
            let body = body.into_any_element();
            dialog
                .title(crate::ui::shell::dialog_title(kind.title()))
                .content(crate::ui::shell::scrollable_dialog_content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
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
                        keys::Q_PEOPLE_PUBLIC_USERNAME_2,
                    )))
                    .on_close(on_close);
            };
            let (title, hint) = match dialog_state.kind {
                TextPromptKind::Username => (
                    tr(keys::Q_PEOPLE_PUBLIC_USERNAME_2),
                    tr(keys::Q_PEOPLE_PUBLIC_LINK_T_ME_USERNAME),
                ),
                TextPromptKind::CustomTitle { .. } => (
                    tr(keys::Q_PEOPLE_CUSTOM_TITLE),
                    tr(keys::Q_PEOPLE_A_TITLE_THAT_MEMBERS_WILL),
                ),
                // Slice G8: group/channel info editing reuses the text
                // prompt — each kind gets its own title and hint.
                TextPromptKind::GroupTitle => (
                    tr(keys::Q_PEOPLE_GROUP_TITLE),
                    tr(keys::Q_PEOPLE_1_128_CHARACTERS),
                ),
                TextPromptKind::GroupDescription => (
                    tr(keys::Q_PEOPLE_GROUP_DESCRIPTION),
                    tr(keys::Q_PEOPLE_UP_TO_255_CHARACTERS_EMPTY),
                ),
                TextPromptKind::GroupPhoto => (
                    tr(keys::MEDIAVIEW_GROUP_PHOTO),
                    tr(keys::Q_PEOPLE_PATH_TO_AN_IMAGE_FILE),
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
                .into_any_element();
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
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit_username_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::Username, window, cx);
                        })),
                );
            dialog
                .title(crate::ui::shell::dialog_title(title))
                .content(crate::ui::shell::scrollable_dialog_content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
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
