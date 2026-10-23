//! Methods moved out of `profile_panels.rs` to keep files under 1000 lines.

use super::*;
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr, tr_with};

impl QuillApp {
    pub(super) fn submit_personal_photo(
        &mut self,
        user_id: i64,
        mode: PersonalPhotoMode,
        path: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live.as_mut() else {
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_PEOPLE_DEMO_MODE_PHOTO_CHANGES_NEED,
            )));
            cx.notify();
            return;
        };
        let sent = match (mode, path) {
            (PersonalPhotoMode::Set, Some(path)) => {
                live.driver.set_user_personal_photo(user_id, Some(path))
            }
            (PersonalPhotoMode::Reset, _) => live.driver.set_user_personal_photo(user_id, None),
            (PersonalPhotoMode::Suggest, Some(path)) => {
                live.driver.suggest_user_photo(user_id, path)
            }
            _ => return,
        };
        self.connection.set_note(match sent {
            Ok(_) => StatusNote::info(mode.done_note()),
            Err(_) => StatusNote::error(tr(keys::Q_PEOPLE_COULDNT_REACH_TELEGRAM_TRY_AGAIN)),
        });
        cx.notify();
    }

    pub(in crate::ui) fn close_profile_dialog(&mut self, cx: &mut Context<Self>) {
        self.dialogs.profile_dialog = None;
        cx.notify();
    }

    /// Edit contact: names, private note and (when the server asks for it)
    /// "Share my phone number".
    pub(in crate::ui) fn open_edit_contact_dialog(
        &mut self,
        user_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((first, last, note)) = self.session().and_then(|s| {
            let user = s.user(user_id)?;
            let note = s
                .user_full_info(user_id)
                .map(|i| i.extras.note.clone())
                .unwrap_or_default();
            Some((user.first_name.clone(), user.last_name.clone(), note))
        }) else {
            return;
        };
        let dialog = EditContactDialog::new(window, cx, user_id, &first, &last, &note);
        dialog
            .first_name_input
            .update(cx, |input, cx| input.focus(window, cx));
        self.dialogs.profile_dialog = Some(ProfileDialog::EditContact(dialog));
        cx.notify();
    }

    /// Save the edit-contact box. Only the note changed: `setUserNote`;
    /// otherwise `addContact` (add-or-edit) with the current note so the
    /// note is not cleared.
    pub(super) fn submit_edit_contact(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(ProfileDialog::EditContact(dialog)) = &self.dialogs.profile_dialog else {
            return false;
        };
        let read = |input: &Entity<TextareaState>| {
            input
                .read(cx)
                .value()
                .replace(['\n', '\r'], " ")
                .trim()
                .to_string()
        };
        let (user_id, first, last, note, share) = (
            dialog.user_id,
            read(&dialog.first_name_input),
            read(&dialog.last_name_input),
            dialog.note_input.read(cx).value().trim().to_string(),
            dialog.share_phone,
        );
        if first.is_empty() && last.is_empty() {
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_PEOPLE_ENTER_A_NAME_FOR_THE)));
            cx.notify();
            return false;
        }
        let Some((phone, old_first, old_last)) = self.session().and_then(|s| {
            s.user(user_id).map(|u| {
                (
                    u.phone_number.clone(),
                    u.first_name.clone(),
                    u.last_name.clone(),
                )
            })
        }) else {
            return false;
        };
        let Some(live) = self.live.as_mut() else {
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_PEOPLE_DEMO_MODE_CONTACT_EDITS_NEED,
            )));
            cx.notify();
            return true;
        };
        let names_unchanged = first == old_first && last == old_last;
        let sent = if names_unchanged && !share {
            live.driver.set_user_note(user_id, &note).map(|_| ())
        } else {
            live.driver
                .edit_contact(user_id, &phone, &first, &last, &note, share)
                .map(|_| ())
        };
        match sent {
            Ok(()) => {
                self.connection
                    .set_note(StatusNote::silent(tr(keys::Q_PEOPLE_CONTACT_SAVED)));
                true
            }
            Err(_) => {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_PEOPLE_COULDNT_REACH_TELEGRAM_TRY_AGAIN,
                )));
                cx.notify();
                false
            }
        }
    }

    pub(in crate::ui) fn open_birthday_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = self
            .session()
            .and_then(|s| s.my_user_id.and_then(|me| s.user_full_info(me)))
            .and_then(|i| i.extras.birthdate)
            .map(|b| (b.day, b.month, b.year));
        self.show_birthday_dialog(current, window, cx);
    }

    /// The suggested-birthday card's "View": the same form, filled with
    /// the suggested date (tdesktop opens its edit box the same way).
    pub(in crate::ui) fn open_suggested_birthday(
        &mut self,
        parts: (u8, u8, Option<i32>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_birthday_dialog(Some(parts), window, cx);
    }

    pub(super) fn show_birthday_dialog(
        &mut self,
        current: Option<(u8, u8, Option<i32>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dialog = BirthdayDialog::new(window, cx, current);
        dialog
            .day_input
            .update(cx, |input, cx| input.focus(window, cx));
        self.dialogs.profile_dialog = Some(ProfileDialog::Birthday(dialog));
        cx.notify();
    }

    /// "Choose who can see your birthday": close the form and open the
    /// Privacy editor for the date-of-birth rule (tdesktop links the same
    /// `Privacy::Key::Birthday` box from the birthday row).
    pub(in crate::ui) fn open_birthday_privacy(&mut self, cx: &mut Context<Self>) {
        self.dialogs.profile_dialog = None;
        self.open_privacy(cx);
        self.privacy.editor = Some(super::super::privacy::PrivacyEditorTarget::Rule(
            quill::telegram::requests_privacy::PrivacySettingKey::ShowBirthdate,
        ));
        cx.notify();
    }

    /// `remove` clears the birthday; otherwise the form is validated first
    /// and a bad value stays in the dialog with its message.
    pub(super) fn submit_birthday(&mut self, remove: bool, cx: &mut Context<Self>) -> bool {
        let parsed = if remove {
            Ok(None)
        } else {
            let Some(ProfileDialog::Birthday(dialog)) = &self.dialogs.profile_dialog else {
                return false;
            };
            let field = |input: &Entity<TextareaState>| input.read(cx).value().to_string();
            let year_now =
                quill::local_time::civil_local(quill::local_time::now_unix()).year as i32;
            parse_birthday(
                &field(&dialog.day_input),
                &field(&dialog.month_input),
                &field(&dialog.year_input),
                year_now,
            )
            .map(Some)
        };
        let parts = match parsed {
            Ok(parts) => parts,
            Err(message) => {
                if let Some(ProfileDialog::Birthday(dialog)) = &mut self.dialogs.profile_dialog {
                    dialog.error = Some(message);
                }
                cx.notify();
                return false;
            }
        };
        let Some(live) = self.live.as_mut() else {
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_PEOPLE_DEMO_MODE_BIRTHDAY_CHANGES_NEED,
            )));
            cx.notify();
            return true;
        };
        match live.driver.set_birthdate(parts) {
            Ok(_) => {
                self.connection.set_note(if remove {
                    StatusNote::silent(tr(keys::Q_PEOPLE_BIRTHDAY_REMOVED))
                } else {
                    StatusNote::silent(tr(keys::Q_PEOPLE_BIRTHDAY_SAVED))
                });
                true
            }
            Err(_) => {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_PEOPLE_COULDNT_REACH_TELEGRAM_TRY_AGAIN,
                )));
                cx.notify();
                false
            }
        }
    }

    pub(in crate::ui) fn open_personal_channel_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.fetch_suitable_personal_chats();
        }
        self.dialogs.profile_dialog = Some(ProfileDialog::PersonalChannel);
        cx.notify();
    }

    pub(super) fn choose_personal_channel(&mut self, chat_id: i64, cx: &mut Context<Self>) -> bool {
        let Some(live) = self.live.as_mut() else {
            self.connection
                .set_note(StatusNote::info(tr(keys::Q_PEOPLE_DEMO_MODE_THIS_NEEDS_A)));
            cx.notify();
            return true;
        };
        match live.driver.set_personal_chat(chat_id) {
            Ok(_) => {
                self.connection.set_note(if chat_id == 0 {
                    StatusNote::silent(tr(keys::Q_PEOPLE_PERSONAL_CHANNEL_REMOVED))
                } else {
                    StatusNote::silent(tr(keys::Q_PEOPLE_PERSONAL_CHANNEL_SAVED))
                });
                true
            }
            Err(_) => {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_PEOPLE_COULDNT_REACH_TELEGRAM_TRY_AGAIN,
                )));
                cx.notify();
                false
            }
        }
    }

    pub(in crate::ui) fn open_share_contact_dialog(
        &mut self,
        user_id: i64,
        cx: &mut Context<Self>,
    ) {
        self.dialogs.profile_dialog = Some(ProfileDialog::ShareContact {
            user_id,
            target: None,
        });
        cx.notify();
    }

    pub(super) fn send_shared_contact(
        &mut self,
        user_id: i64,
        chat_id: i64,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(live) = self.live.as_mut() else {
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_PEOPLE_DEMO_MODE_SHARING_NEEDS_A,
            )));
            cx.notify();
            return true;
        };
        let title = live
            .driver
            .session
            .chats
            .get(&chat_id)
            .map(|c| c.title.clone())
            .unwrap_or_default();
        match live.driver.send_contact_message(ChatId(chat_id), user_id) {
            Ok(_) => {
                self.connection.set_note(StatusNote::silent(tr_with(
                    keys::Q_PEOPLE_CONTACT_SHARED_WITH,
                    &[("title", &title.to_string())],
                )));
                true
            }
            Err(_) => {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_PEOPLE_COULDNT_SHARE_THE_CONTACT_HERE,
                )));
                cx.notify();
                false
            }
        }
    }

    /// Title, body and footer of the open profile dialog.
    pub(super) fn profile_dialog_parts(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<(String, AnyElement, AnyElement)> {
        let muted = cx.theme().muted_foreground;
        let labeled = |label: &'static str, input: &Entity<TextareaState>| {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_xs().text_color(muted).child(label))
                .child(Textarea::new(input).aria_label(label).h(px(40.)))
        };
        let cancel = |id: &'static str, label: &'static str, cx: &mut Context<Self>| {
            Button::new(id)
                .label(label)
                .ghost()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.close_profile_dialog(cx);
                    this.close_kit_dialog_if_done(DialogKind::ProfilePanel, window, cx);
                }))
        };
        if matches!(
            self.dialogs.profile_dialog,
            Some(ProfileDialog::AddBot { .. })
        ) {
            return self.add_bot_dialog_parts(cx);
        }
        if matches!(
            self.dialogs.profile_dialog,
            Some(ProfileDialog::ShareGame { .. })
        ) {
            return self.share_game_dialog_parts(cx);
        }
        match self.dialogs.profile_dialog.as_ref()? {
            ProfileDialog::AddBot { .. } | ProfileDialog::ShareGame { .. } => None,
            ProfileDialog::EditContact(dialog) => {
                let user_id = dialog.user_id;
                let (phone, show_share, name) = self
                    .session()
                    .map(|s| {
                        let user = s.user(user_id);
                        (
                            user.map(|u| u.phone_number.clone()).unwrap_or_default(),
                            s.user_full_info(user_id)
                                .is_some_and(|i| i.extras.need_phone_exception),
                            user.map(|u| u.first_name.clone()).unwrap_or_default(),
                        )
                    })
                    .unwrap_or_default();
                let share_phone = dialog.share_phone;
                let mut body = div().flex().flex_col().gap_3();
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(if phone.is_empty() {
                            tr(keys::Q_PEOPLE_PHONE_NUMBER_HIDDEN).to_string()
                        } else {
                            format_phone(&phone)
                        }),
                );
                body = body
                    .child(labeled(
                        tr(keys::Q_PEOPLE_FIRST_NAME),
                        &dialog.first_name_input,
                    ))
                    .child(labeled(
                        tr(keys::Q_PEOPLE_LAST_NAME),
                        &dialog.last_name_input,
                    ))
                    .child(labeled(tr(keys::Q_PEOPLE_NOTE), &dialog.note_input))
                    .children(self.contact_photo_rows(
                        user_id,
                        super::contact_photo_rows::PhotoRowsHost::EditContact,
                        cx,
                    ));
                if show_share {
                    body = body
                        .child(
                            Checkbox::new("edit-contact-share-phone")
                                .checked(share_phone)
                                .label(tr(keys::NEW_CONTACT_SHARE))
                                .on_click(cx.listener(|this, &on, window, cx| {
                                    if let Some(ProfileDialog::EditContact(dialog)) =
                                        &mut this.dialogs.profile_dialog
                                    {
                                        dialog.share_phone = on;
                                    }
                                    window.refresh();
                                    cx.notify();
                                })),
                        )
                        .child(div().text_xs().text_color(muted).child(format!(
                            "{} will be able to see your phone number.",
                            if name.is_empty() {
                                tr(keys::Q_PEOPLE_THE_CONTACT)
                            } else {
                                &name
                            }
                        )));
                }
                let footer = div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("edit-contact-save")
                            .label(tr(keys::SETTINGS_SAVE))
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.submit_edit_contact(cx) {
                                    this.close_profile_dialog(cx);
                                }
                                this.close_kit_dialog_if_done(DialogKind::ProfilePanel, window, cx);
                            })),
                    )
                    .child(cancel("edit-contact-cancel", tr(keys::CANCEL), cx));
                Some((
                    tr(keys::INFO_EDIT_CONTACT).into(),
                    body.into_any_element(),
                    footer.into_any_element(),
                ))
            }
            ProfileDialog::Birthday(dialog) => {
                let has_birthday = self
                    .session()
                    .and_then(|s| s.my_user_id.and_then(|me| s.user_full_info(me)))
                    .is_some_and(|i| i.extras.birthdate.is_some());
                let mut body = div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .child(labeled(tr(keys::Q_PEOPLE_DAY), &dialog.day_input)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .child(labeled(tr(keys::Q_PEOPLE_MONTH), &dialog.month_input)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .child(labeled(tr(keys::Q_PEOPLE_YEAR), &dialog.year_input)),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(tr(keys::Q_PEOPLE_THE_YEAR_IS_OPTIONAL_YOUR)),
                    )
                    .child(
                        div()
                            .id("birthday-privacy-link")
                            .role(gpui_kit::Role::Button)
                            .aria_label(tr(keys::Q_PEOPLE_CHANGE_WHO_CAN_SEE_YOUR))
                            .tab_index(0)
                            .cursor_pointer()
                            .text_sm()
                            .text_color(accent())
                            .child(tr(keys::Q_PEOPLE_CHOOSE_WHO_CAN_SEE_YOUR))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_birthday_privacy(cx);
                                this.close_kit_dialog_if_done(DialogKind::ProfilePanel, window, cx);
                            })),
                    );
                if let Some(error) = dialog.error {
                    body = body.child(
                        div()
                            .id("birthday-error")
                            .text_sm()
                            .text_color(cx.theme().danger)
                            .child(error),
                    );
                }
                let mut footer = div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("birthday-save")
                            .label(tr(keys::SETTINGS_SAVE))
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.submit_birthday(false, cx) {
                                    this.close_profile_dialog(cx);
                                }
                                this.close_kit_dialog_if_done(DialogKind::ProfilePanel, window, cx);
                            })),
                    )
                    .child(cancel("birthday-cancel", tr(keys::CANCEL), cx));
                if has_birthday {
                    footer = footer.child(
                        Button::new("birthday-remove")
                            .label(tr(keys::Q_PEOPLE_REMOVE_BIRTHDAY))
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.submit_birthday(true, cx) {
                                    this.close_profile_dialog(cx);
                                }
                                this.close_kit_dialog_if_done(DialogKind::ProfilePanel, window, cx);
                            })),
                    );
                }
                Some((
                    tr(keys::Q_PEOPLE_YOUR_BIRTHDAY).into(),
                    body.into_any_element(),
                    footer.into_any_element(),
                ))
            }
            ProfileDialog::PersonalChannel => {
                let session = self.session()?;
                let current = session
                    .my_user_id
                    .and_then(|me| session.user_full_info(me))
                    .map_or(0, |i| i.extras.personal_chat_id);
                let fetch = session
                    .users_state
                    .profile_chat_lists
                    .get(&(ProfileChatsKind::SuitablePersonalChats, 0))
                    .cloned();
                let mut body = div().flex().flex_col().gap_1().child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .pb_1()
                        .child(tr(keys::Q_PEOPLE_SHOW_A_CHANNEL_ON_YOUR)),
                );
                match fetch {
                    None | Some(ProfileChatsFetch::Loading) => {
                        body = body.child(
                            div()
                                .text_sm()
                                .text_color(muted)
                                .child(tr(keys::PROFILE_LOADING)),
                        );
                    }
                    Some(ProfileChatsFetch::Failed(_)) => {
                        body = body.child(
                            action_row(
                                "personal-channel-retry",
                                Some(IconName::RotateCw),
                                tr(keys::Q_PEOPLE_COULDNT_LOAD_CHANNELS_RETRY),
                                false,
                                cx,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.retry_profile_chats(
                                    ProfileChatsKind::SuitablePersonalChats,
                                    0,
                                    cx,
                                );
                            })),
                        );
                    }
                    Some(ProfileChatsFetch::Loaded(ids)) if ids.is_empty() => {
                        body = body.child(
                            div()
                                .text_sm()
                                .text_color(muted)
                                .child(tr(keys::Q_PEOPLE_YOU_DONT_HAVE_ANY_CHANNELS)),
                        );
                    }
                    Some(ProfileChatsFetch::Loaded(ids)) => {
                        for id in ids {
                            let Some(title) = session.chats.get(&id).map(|c| c.title.clone())
                            else {
                                continue;
                            };
                            let selected = id == current;
                            body = body.child(
                                action_row(
                                    ("personal-channel-choice", id.unsigned_abs()),
                                    selected.then_some(IconName::CircleCheck),
                                    title,
                                    false,
                                    cx,
                                )
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        if this.choose_personal_channel(id, cx) {
                                            this.close_profile_dialog(cx);
                                        }
                                        this.close_kit_dialog_if_done(
                                            DialogKind::ProfilePanel,
                                            window,
                                            cx,
                                        );
                                    },
                                )),
                            );
                        }
                    }
                }
                let mut footer = div().flex().gap_2();
                if current != 0 {
                    footer = footer.child(
                        Button::new("personal-channel-remove")
                            .label(tr(keys::Q_PEOPLE_REMOVE_PERSONAL_CHANNEL))
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.choose_personal_channel(0, cx) {
                                    this.close_profile_dialog(cx);
                                }
                                this.close_kit_dialog_if_done(DialogKind::ProfilePanel, window, cx);
                            })),
                    );
                }
                footer = footer.child(cancel("personal-channel-cancel", tr(keys::CLOSE), cx));
                Some((
                    tr(keys::Q_PEOPLE_PERSONAL_CHANNEL).into(),
                    body.into_any_element(),
                    footer.into_any_element(),
                ))
            }
            ProfileDialog::MainTab => self.main_tab_dialog_parts(cx),
            ProfileDialog::ChangePhone => self.change_phone_dialog_parts(cx),
            ProfileDialog::ShareContact { user_id, target } => {
                let (user_id, target) = (*user_id, *target);
                let session = self.session()?;
                let name = session
                    .user(user_id)
                    .map(|u| u.display_name())
                    .unwrap_or_default();
                let chat_title = |id: i64| {
                    session
                        .chats
                        .get(&id)
                        .map(|c| c.title.clone())
                        .unwrap_or_default()
                };
                if let Some(chat_id) = target {
                    let body = div().text_sm().child(tr_with(
                        keys::Q_LEFTOVERS_SHARE_CONTACT_WITH,
                        &[
                            ("name", &name.to_string()),
                            ("chat", &(chat_title(chat_id)).to_string()),
                        ],
                    ));
                    let footer = div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("share-contact-send")
                                .label(tr(keys::Q_PEOPLE_SEND))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if this.send_shared_contact(user_id, chat_id, cx) {
                                        this.close_profile_dialog(cx);
                                    }
                                    this.close_kit_dialog_if_done(
                                        DialogKind::ProfilePanel,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            Button::new("share-contact-back")
                                .label(tr(keys::CREATE_GROUP_BACK))
                                .ghost()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.dialogs.profile_dialog =
                                        Some(ProfileDialog::ShareContact {
                                            user_id,
                                            target: None,
                                        });
                                    window.refresh();
                                    cx.notify();
                                })),
                        );
                    return Some((
                        tr(keys::Q_PEOPLE_SHARE_CONTACT).into(),
                        body.into_any_element(),
                        footer.into_any_element(),
                    ));
                }
                let chats: Vec<(i64, String)> = session
                    .forward_destinations("")
                    .into_iter()
                    .filter(|chat| chat.can_post())
                    .take(60)
                    .map(|chat| (chat.id.0, chat.title.clone()))
                    .collect();
                let mut body = div().flex().flex_col().gap_1().child(
                    div().text_xs().text_color(muted).pb_1().child(tr_with(
                        keys::Q_PEOPLE_CHOOSE_WHERE_TO_SEND_S,
                        &[("name", &name.to_string())],
                    )),
                );
                if chats.is_empty() {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(tr(keys::Q_PEOPLE_NO_CHATS_LOADED_YET)),
                    );
                }
                for (id, title) in chats {
                    body = body.child(
                        action_row(
                            ("share-contact-chat", id.unsigned_abs()),
                            None,
                            title,
                            false,
                            cx,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.dialogs.profile_dialog = Some(ProfileDialog::ShareContact {
                                    user_id,
                                    target: Some(id),
                                });
                                window.refresh();
                                cx.notify();
                            },
                        )),
                    );
                }
                let footer = div().flex().gap_2().child(cancel(
                    "share-contact-cancel",
                    tr(keys::CANCEL),
                    cx,
                ));
                Some((
                    tr(keys::Q_PEOPLE_SHARE_CONTACT).into(),
                    body.into_any_element(),
                    footer.into_any_element(),
                ))
            }
            ProfileDialog::PersonalPhoto {
                user_id,
                mode,
                path,
            } => {
                let (user_id, mode, path) = (*user_id, *mode, path.clone());
                let name = self
                    .session()
                    .and_then(|s| s.user(user_id))
                    .map(|u| u.display_name())
                    .unwrap_or_default();
                let body = div().text_sm().child(mode.confirm_text(&name));
                let footer = div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("personal-photo-confirm")
                            .label(mode.button())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.submit_personal_photo(user_id, mode, path.as_deref(), cx);
                                this.close_profile_dialog(cx);
                                this.close_kit_dialog_if_done(DialogKind::ProfilePanel, window, cx);
                            })),
                    )
                    .child(cancel("personal-photo-cancel", tr(keys::CANCEL), cx));
                Some((
                    mode.title().into(),
                    body.into_any_element(),
                    footer.into_any_element(),
                ))
            }
            ProfileDialog::ReportPhoto { owner, file_id } => {
                let (owner, file_id) = (*owner, *file_id);
                let mut body = div().flex().flex_col().gap_1().child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .pb_1()
                        .child(tr(keys::Q_PEOPLE_WHY_ARE_YOU_REPORTING_THIS)),
                );
                for reason in PhotoReportReason::ALL {
                    body = body.child(
                        action_row(
                            ("report-photo-reason", reason as u64),
                            None,
                            reason.label(),
                            false,
                            cx,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.submit_photo_report(owner, file_id, reason, cx);
                                this.close_profile_dialog(cx);
                                this.close_kit_dialog_if_done(DialogKind::ProfilePanel, window, cx);
                            },
                        )),
                    );
                }
                let footer =
                    div()
                        .flex()
                        .gap_2()
                        .child(cancel("report-photo-cancel", tr(keys::CANCEL), cx));
                Some((
                    tr(keys::REPORT_BUTTON).into(),
                    body.into_any_element(),
                    footer.into_any_element(),
                ))
            }
        }
    }

    /// The kit `Dialog` host for every profile panel dialog.
    pub(in crate::ui) fn build_profile_panel_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ProfilePanel, |this, _, cx| {
                this.close_profile_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let Some((title, body, footer)) = this.profile_dialog_parts(cx) else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title(tr(
                        keys::SETTINGS_COLOR_TAB_PROFILE,
                    )))
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
