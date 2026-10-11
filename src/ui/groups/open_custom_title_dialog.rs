//! Methods moved out of `groups.rs` to keep files under 1000 lines.

use super::*;

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
                            "username removed".into()
                        } else {
                            format!("username set to @{value}")
                        }
                    }
                    Err(_) => {
                        self.admin.username_dialog = Some(dialog);
                        "could not set username".into()
                    }
                },
                None => {
                    self.admin.username_dialog = Some(dialog);
                    "usernames need a live connection (demo)".into()
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
                        "custom title must be at most 16 characters".into()
                    } else {
                        "custom title cannot contain emoji".into()
                    };
                    cx.notify();
                    return;
                }
                match self.live.as_mut() {
                    Some(live) => match live.driver.set_chat_member_tag(chat_id, user_id, &value) {
                        Ok(_) => {
                            if value.is_empty() {
                                "custom title removed".into()
                            } else {
                                "custom title updated".into()
                            }
                        }
                        Err(_) => {
                            self.admin.username_dialog = Some(dialog);
                            "could not set custom title".into()
                        }
                    },
                    None => {
                        self.admin.username_dialog = Some(dialog);
                        "custom titles need a live connection (demo)".into()
                    }
                }
            }
            TextPromptKind::GroupTitle => {
                // 1–128 chars per the schema (line 13430); the driver
                // re-validates before sending.
                if value.is_empty() || value.chars().count() > 128 {
                    self.admin.username_dialog = Some(dialog);
                    self.connection.status_note = "title must be 1–128 characters".into();
                    cx.notify();
                    return;
                }
                match self.live.as_mut() {
                    Some(live) => match live.driver.set_group_title(chat_id, &value) {
                        Ok(Some(_)) => "title updated".into(),
                        Ok(None) => {
                            self.admin.username_dialog = Some(dialog);
                            "you can't change this group's info".into()
                        }
                        Err(_) => {
                            self.admin.username_dialog = Some(dialog);
                            "could not update title".into()
                        }
                    },
                    None => {
                        self.admin.username_dialog = Some(dialog);
                        "titles need a live connection (demo)".into()
                    }
                }
            }
            // Slice G10: `setCommunityName` (schema 1.8.67, line 11811;
            // the driver refuses empty names client-side). Not
            // optimistic — the new name arrives via `updateCommunity`.
            TextPromptKind::CommunityName { community_id } => {
                if value.is_empty() {
                    self.admin.username_dialog = Some(dialog);
                    self.connection.status_note = "community name cannot be empty".into();
                    cx.notify();
                    return;
                }
                match self.live.as_mut() {
                    Some(live) => match live.driver.set_community_name(community_id, &value) {
                        Ok(_) => "community name updated".into(),
                        Err(_) => {
                            self.admin.username_dialog = Some(dialog);
                            "could not update community name".into()
                        }
                    },
                    None => {
                        self.admin.username_dialog = Some(dialog);
                        "renaming communities needs a live connection (demo)".into()
                    }
                }
            }
            TextPromptKind::GroupDescription => {
                // 0–255 chars per the schema (line 13533); empty clears.
                if value.chars().count() > 255 {
                    self.admin.username_dialog = Some(dialog);
                    self.connection.status_note =
                        "description must be at most 255 characters".into();
                    cx.notify();
                    return;
                }
                match self.live.as_mut() {
                    Some(live) => match live.driver.set_group_description(chat_id, &value) {
                        Ok(Some(_)) => {
                            if value.is_empty() {
                                "description cleared".into()
                            } else {
                                "description updated".into()
                            }
                        }
                        Ok(None) => {
                            self.admin.username_dialog = Some(dialog);
                            "you can't change this group's info".into()
                        }
                        Err(_) => {
                            self.admin.username_dialog = Some(dialog);
                            "could not update description".into()
                        }
                    },
                    None => {
                        self.admin.username_dialog = Some(dialog);
                        "descriptions need a live connection (demo)".into()
                    }
                }
            }
            TextPromptKind::GroupPhoto => {
                // Empty removes the photo; otherwise the path must be
                // a real file — TDLib would reject a missing one anyway.
                // `~` expands to the home dir.
                let value = if let Some(rest) = value.strip_prefix('~') {
                    format!("{}{}", std::env::var("HOME").unwrap_or_default(), rest)
                } else {
                    value
                };
                let photo = if value.is_empty() {
                    None
                } else if std::path::Path::new(&value).is_file() {
                    Some(value.as_str())
                } else {
                    self.admin.username_dialog = Some(dialog);
                    self.connection.status_note = format!("file not found: {value}");
                    cx.notify();
                    return;
                };
                match self.live.as_mut() {
                    Some(live) => match live.driver.set_group_photo(chat_id, photo) {
                        Ok(Some(_)) => {
                            if value.is_empty() {
                                "photo removed".into()
                            } else {
                                "photo updated".into()
                            }
                        }
                        Ok(None) => {
                            self.admin.username_dialog = Some(dialog);
                            "you can't change this group's info".into()
                        }
                        Err(_) => {
                            self.admin.username_dialog = Some(dialog);
                            "could not update photo".into()
                        }
                    },
                    None => {
                        self.admin.username_dialog = Some(dialog);
                        "photos need a live connection (demo)".into()
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
            .unwrap_or_else(|| format!("User {user_id}"))
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
                "request already in flight".to_string()
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
        let note = match self.live.as_mut() {
            Some(live) => {
                let result = match dialog.action {
                    GroupConfirmAction::DeleteChat => live
                        .driver
                        .delete_chat(dialog.chat_id)
                        .map(|sent| sent_note(sent, "chat deleted")),
                    GroupConfirmAction::LeaveChat => live
                        .driver
                        .leave_channel(dialog.chat_id)
                        .map(|_| "left the chat".to_string()),
                    // Handled above; never reaches the driver.
                    GroupConfirmAction::BroadcastIntro => Ok("".to_string()),
                    GroupConfirmAction::BroadcastUpgrade => live
                        .driver
                        .upgrade_to_broadcast_group(dialog.chat_id)
                        // Ongoing, not done: TDLib answers `ok`/`error`
                        // asynchronously; the error arm rolls the
                        // optimistic flag back.
                        .map(|sent| sent_note(sent, "converting to broadcast group…")),
                    // Slice CL1: `deleteChatHistory` (schema 1.8.67,
                    // line 11845). TDLib answers `ok`/`error`
                    // asynchronously; a refusal surfaces via
                    // `Session::chat_action_error`.
                    GroupConfirmAction::ClearHistory { revoke } => live
                        .driver
                        .clear_chat_history(dialog.chat_id, revoke)
                        .map(|sent| sent_note(sent, "clearing history…")),
                    // Slice B2: "Restart bot" — `deleteChatHistory` +
                    // `sendBotStartMessage` (schema lines 11845 / 12216).
                    // The bot user id is read from the session before the
                    // live borrow; a missing id fails closed with a status
                    // note, never silently.
                    GroupConfirmAction::RestartBot => match restart_bot_user_id {
                        Some(bot_user_id) => live
                            .driver
                            .restart_bot(dialog.chat_id, bot_user_id)
                            .map(|sent| sent_note(sent, "restarting bot…")),
                        None => Ok("This chat is no longer a bot chat.".to_string()),
                    },
                    // Slice CL1: chat-list "Delete chat" —
                    // `deleteChatHistory` with `remove_from_chat_list:
                    // true` (Telegram X `Tdlib.deleteChat`).
                    GroupConfirmAction::RemoveFromList => live
                        .driver
                        .remove_chat_from_list(dialog.chat_id)
                        .map(|sent| sent_note(sent, "deleting chat…")),
                    // Slice CL3: row-menu Report (`reportChat`, schema
                    // 1.8.67, line 15693). TDLib answers
                    // `ReportChatResult` asynchronously; the outcome
                    // surfaces via `Session::report_chat_outcome`.
                    GroupConfirmAction::ReportChat => live
                        .driver
                        .report_chat(dialog.chat_id)
                        .map(|sent| sent_note(sent, "reporting chat…")),
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
                                    "blocking user…"
                                } else {
                                    "unblocking user…"
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
                                    "blocking user…"
                                } else {
                                    "unblocking user…"
                                },
                            )
                        })
                    }
                    // Slice A6: user-panel "Delete contact" —
                    // `removeContacts` (schema 1.8.67, line 14528).
                    GroupConfirmAction::DeleteContact { user_id } => live
                        .driver
                        .remove_contact(user_id)
                        .map(|sent| sent_note(sent, "deleting contact…")),
                    // Slice A6: "Delete synced contacts" —
                    // `clearImportedContacts` + `removeContacts`.
                    GroupConfirmAction::DeleteSyncedContacts => {
                        live.driver.delete_synced_contacts().map(|sent| {
                            if sent > 0 {
                                "deleting synced contacts…".to_string()
                            } else {
                                "nothing to delete".to_string()
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
                        let chat_ids: Vec<ChatId> = self
                            .chat_list
                            .selected
                            .iter()
                            .map(|id| ChatId(*id))
                            .collect();
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
                        Ok(format!("deleting {sent} of {total} chats…"))
                    }
                    // Slice A2: abort the pending recovery-email setup
                    // (`cancelRecoveryEmailAddressVerification`, schema
                    // 1.8.67, line 11467). The dialog carries no chat, so
                    // `dialog.chat_id` is unused here.
                    GroupConfirmAction::AbortRecoveryEmailSetup => live
                        .driver
                        .cancel_recovery_email_setup()
                        .map(|_| "aborting email setup…".to_string()),
                    GroupConfirmAction::RemoveSavedGif { file_id } => live
                        .driver
                        .set_gif_saved(file_id, false)
                        .map(|id| sent_note(id, "removing saved GIF…")),
                    GroupConfirmAction::DeleteForumTopic { forum_topic_id } => live
                        .driver
                        .delete_forum_topic(dialog.chat_id, forum_topic_id)
                        .map(|_| "deleting topic…".to_string()),
                    GroupConfirmAction::DeleteSavedSublist { topic_id } => live
                        .driver
                        .delete_saved_topic_history(topic_id)
                        .map(|_| "deleting saved messages…".to_string()),
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
                        live.driver
                            .manage_sticker_sets(&ids, false)
                            .map(|sent| format!("removing {sent} sticker sets…"))
                    }
                    GroupConfirmAction::RemoveEmojiSet { set_id } => live
                        .driver
                        .set_emoji_pack_installed(set_id, false)
                        .map(|id| sent_note(id, "removing emoji pack…")),
                    GroupConfirmAction::RemoveStickerSet { set_id } => live
                        .driver
                        .manage_sticker_set(set_id, false, false)
                        .map(|id| sent_note(id, "removing sticker set…")),
                    // Clearing stored payment information waits for TDLib confirmation.
                    GroupConfirmAction::ClearPaymentInfo => live
                        .driver
                        .clear_saved_payment_info()
                        .map(|_| "clearing saved payment info…".to_string()),
                    // `deleteCommunity` (TDLib 1.8.68). The state drops the
                    // community on `ok`; a refusal surfaces through
                    // `Session::community_error`.
                    GroupConfirmAction::DeleteCommunity { community_id } => live
                        .driver
                        .delete_community(community_id)
                        .map(|sent| sent_note(sent, "deleting community…")),
                    GroupConfirmAction::RemoveMember { user_id } => live
                        .driver
                        .remove_chat_member(dialog.chat_id, user_id)
                        .map(|id| sent_note(id, "removing member…")),
                };
                match result {
                    Ok(note) => {
                        if matches!(dialog.action, GroupConfirmAction::LeaveChat) {
                            self.note_left_channel(dialog.chat_id, cx);
                        }
                        note
                    }
                    Err(_) => {
                        self.admin.group_confirm_dialog = Some(dialog);
                        "action failed".to_string()
                    }
                }
            }
            None => {
                self.admin.group_confirm_dialog = Some(dialog);
                "chat actions need a live connection (demo)".to_string()
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
                    .title(crate::ui::shell::dialog_title("New chat"))
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
                            .aria_label("Group or channel title")
                            .h(px(40.)),
                    ),
                )
                .when(kind != CreateChatKind::BasicGroup, |this| {
                    this.child(
                        div().flex_1().child(
                            Textarea::new(&dialog_state.description_input)
                                .aria_label("Group or channel description")
                                .h(px(64.)),
                        ),
                    )
                });
            if picks_members {
                body = body.child(
                    div().flex().items_center().gap_2().child(
                        div().flex_1().child(
                            Textarea::new(&dialog_state.search_input)
                                .aria_label("Search people to invite")
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
                            .child("No contacts found"),
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
                            .child(format!("{} members selected", selected.len())),
                    );
                }
            }
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("g1-create-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_create_chat_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::CreateChat, window, cx);
                        })),
                )
                .child(
                    Button::new("g1-create-submit")
                        .label(format!("Create {}", kind.title().to_lowercase()))
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
                    .title(crate::ui::shell::dialog_title("Public username"))
                    .on_close(on_close);
            };
            let (title, hint) = match dialog_state.kind {
                TextPromptKind::Username => (
                    "Public username",
                    "Public link t.me/username — empty removes it",
                ),
                TextPromptKind::CustomTitle { .. } => (
                    "Custom title",
                    "A title that members will see instead of 'Admin'. Empty removes it.",
                ),
                // Slice G8: group/channel info editing reuses the text
                // prompt — each kind gets its own title and hint.
                TextPromptKind::GroupTitle => ("Group title", "1–128 characters"),
                TextPromptKind::GroupDescription => (
                    "Group description",
                    "Up to 255 characters — empty clears it",
                ),
                TextPromptKind::GroupPhoto => (
                    "Group photo",
                    "Path to an image file — empty removes the photo",
                ),
                // Slice G10: community rename prompt.
                TextPromptKind::CommunityName { .. } => (
                    "Community name",
                    "Shown in the communities hub and info panel",
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
                                    .aria_label("Public username")
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
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_username_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::Username, window, cx);
                        })),
                )
                .child(
                    Button::new("g1-username-submit")
                        .label("Save")
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
