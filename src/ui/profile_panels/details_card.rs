//! The details card of a user profile: bio, phone, username, link,
//! birthday, personal channel and the private note.
#![deny(clippy::indexing_slicing)]

use super::*;

/// What a tap on a detail row does.
#[derive(Clone)]
enum RowAction {
    /// Only copy the value (the row's copy action).
    None,
    /// Open the birthday form (your own profile).
    EditBirthday,
    /// Open the personal channel picker (your own profile).
    PickPersonalChannel,
    /// Open the main tab chooser (your own profile).
    PickMainTab,
    /// Open a chat.
    OpenChat(ChatId),
    /// Open the edit-contact box focused on the note.
    EditNote(i64),
    /// A collectible username or phone number: the box with where and for
    /// how much it was bought (tdesktop `internal:collectible_*` links).
    Collectible { entity: String, copy_text: String },
}

/// One value-over-label row of the profile details card.
struct DetailRow {
    id: &'static str,
    value: String,
    label: String,
    copy: Option<CopyAction>,
    action: RowAction,
    /// Muted value (an "Add …" call to action on your own profile).
    hint: bool,
    /// A second right-click item (the username row also copies its link).
    extra_copy: Option<CopyAction>,
}

/// The `+digits` form copied from the phone row.
fn copyable_phone(raw: &str) -> String {
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    format!("+{digits}")
}

impl QuillApp {
    /// The details card of a user profile: bio, phone, username, link,
    /// birthday, personal channel and the private note. Rows copy their
    /// value on tap and in the right-click menu; the birthday and personal
    /// channel rows of your own profile open their editors instead.
    pub(in crate::ui) fn profile_details_card(
        &self,
        user_id: i64,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let user = session.user(user_id)?;
        let info = session.user_full_info(user_id);
        let is_self = session.my_user_id == Some(user_id);
        let mut rows: Vec<DetailRow> = Vec::new();
        if let Some(bio) = info.map(|i| i.bio.clone()).filter(|b| !b.is_empty()) {
            rows.push(DetailRow {
                id: "info-bio",
                copy: Some(CopyAction {
                    text: bio.clone(),
                    menu: tr(keys::Q_PEOPLE_COPY_BIO),
                    toast: tr(keys::Q_PEOPLE_BIO_COPIED_TO_CLIPBOARD),
                }),
                value: bio,
                label: tr(keys::INFO_BIO_LABEL).into(),
                action: RowAction::None,
                hint: false,
                extra_copy: None,
            });
        }
        // tdesktop `PhoneOrHiddenValue`: a collectible number is a link to
        // its box; so is a username that cannot be edited.
        let collectible_phone = session
            .payments
            .hub
            .fragment
            .is_collectible_phone(&user.phone_number);
        if !user.phone_number.is_empty() {
            rows.push(DetailRow {
                id: "info-phone",
                value: format_phone(&user.phone_number),
                label: tr(keys::INFO_MOBILE_LABEL).into(),
                copy: Some(CopyAction {
                    text: copyable_phone(&user.phone_number),
                    menu: tr(keys::PROFILE_COPY_PHONE),
                    toast: tr(keys::Q_PEOPLE_PHONE_NUMBER_COPIED_TO_CLIPBOARD),
                }),
                action: if collectible_phone {
                    RowAction::Collectible {
                        entity: copyable_phone(&user.phone_number),
                        copy_text: String::new(),
                    }
                } else {
                    RowAction::None
                },
                hint: false,
                extra_copy: None,
            });
        }
        if !user.username.is_empty() {
            rows.push(DetailRow {
                id: "info-username",
                value: format!("@{}", user.username),
                label: tr(keys::INFO_USERNAME_LABEL).into(),
                copy: Some(CopyAction {
                    text: format!("@{}", user.username),
                    menu: tr(keys::Q_PEOPLE_COPY_USERNAME),
                    toast: tr(keys::Q_PEOPLE_USERNAME_COPIED_TO_CLIPBOARD),
                }),
                action: if user.collectible_usernames.contains(&user.username) {
                    RowAction::Collectible {
                        entity: user.username.clone(),
                        copy_text: profile_link(&user.username),
                    }
                } else {
                    RowAction::None
                },
                hint: false,
                extra_copy: Some(CopyAction {
                    text: profile_link(&user.username),
                    menu: tr(keys::CONTEXT_COPY_LINK),
                    toast: tr(keys::STAR_REF_LINK_COPIED_TITLE),
                }),
            });
            let link = profile_link(&user.username);
            rows.push(DetailRow {
                id: "info-link",
                value: link.trim_start_matches("https://").to_string(),
                label: tr(keys::INFO_LINK_LABEL).into(),
                copy: Some(CopyAction {
                    text: link,
                    menu: tr(keys::CONTEXT_COPY_LINK),
                    toast: tr(keys::STAR_REF_LINK_COPIED_TITLE),
                }),
                action: RowAction::None,
                hint: false,
                extra_copy: None,
            });
        }
        match info.and_then(|i| i.extras.birthdate) {
            Some(birthday) => rows.push(DetailRow {
                id: "info-birthday",
                value: crate::ui::group_panels::format_birthday(birthday),
                label: tr(keys::Q_PEOPLE_BIRTHDAY).into(),
                copy: None,
                action: if is_self {
                    RowAction::EditBirthday
                } else {
                    RowAction::None
                },
                hint: false,
                extra_copy: None,
            }),
            None if is_self => rows.push(DetailRow {
                id: "info-birthday",
                value: tr(keys::Q_PEOPLE_ADD_YOUR_BIRTHDAY).into(),
                label: tr(keys::Q_PEOPLE_BIRTHDAY).into(),
                copy: None,
                action: RowAction::EditBirthday,
                hint: true,
                extra_copy: None,
            }),
            None => {}
        }
        let personal = info.map_or(0, |i| i.extras.personal_chat_id);
        let personal_card = (personal != 0)
            .then(|| session.chats.get(&personal))
            .flatten()
            .map(|chat| {
                let subscribers = session
                    .group_member_counts(chat)
                    .map_or(0, |(members, _)| members);
                quill::personal_channel::card(&chat.title, subscribers, &chat.last_preview)
            });
        match personal_card {
            Some(card) => rows.push(DetailRow {
                id: "info-personal-channel",
                value: card.value,
                label: card.label,
                copy: None,
                action: if is_self {
                    RowAction::PickPersonalChannel
                } else {
                    RowAction::OpenChat(ChatId(personal))
                },
                hint: false,
                extra_copy: None,
            }),
            None if is_self => rows.push(DetailRow {
                id: "info-personal-channel",
                value: tr(keys::Q_PEOPLE_ADD_A_PERSONAL_CHANNEL).into(),
                label: tr(keys::Q_PEOPLE_PERSONAL_CHANNEL).into(),
                copy: None,
                action: RowAction::PickPersonalChannel,
                hint: true,
                extra_copy: None,
            }),
            None => {}
        }
        if is_self {
            let current = info.and_then(|i| i.extras.main_profile_tab);
            rows.push(DetailRow {
                id: "info-main-tab",
                value: current
                    .map_or(tr(keys::Q_PEOPLE_DEFAULT), |tab| tab.label())
                    .into(),
                label: tr(keys::Q_PEOPLE_MAIN_TAB).into(),
                copy: None,
                action: RowAction::PickMainTab,
                hint: current.is_none(),
                extra_copy: None,
            });
        }
        if let Some(note) = info
            .map(|i| i.extras.note.clone())
            .filter(|n| !n.is_empty())
        {
            rows.push(DetailRow {
                id: "info-note",
                copy: Some(CopyAction {
                    text: note.clone(),
                    menu: tr(keys::Q_PEOPLE_COPY_NOTE),
                    toast: tr(keys::Q_PEOPLE_NOTE_COPIED_TO_CLIPBOARD),
                }),
                value: note,
                label: tr(keys::Q_PEOPLE_NOTE_ONLY_VISIBLE_TO_YOU).into(),
                action: if user.is_contact {
                    RowAction::EditNote(user_id)
                } else {
                    RowAction::None
                },
                hint: false,
                extra_copy: None,
            });
        }
        let mut business_rows = self.business_rows(user_id, cx);
        business_rows.extend(self.ton_wallet_profile_row(user_id, cx));
        if rows.is_empty() && business_rows.is_empty() {
            return None;
        }
        // tdesktop `AddPhoneMenu`: the note needs the Fragment address from
        // the app config.
        let fragment = &session.payments.hub.fragment;
        let fragment_url = (!is_self && fragment.is_collectible_phone(&user.phone_number))
            .then(|| fragment.link.clone())
            .flatten();
        // tdesktop `AddQrButton`: on the username row, or on your own
        // number when you have no username.
        let qr_row = self
            .qr_eligible(quill::peer_qr::state::QrPeer::User(user_id))
            .then_some(if user.username.is_empty() {
                "info-phone"
            } else {
                "info-username"
            });
        let mut card = div()
            .flex()
            .flex_col()
            .w_full()
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().border);
        let muted = cx.theme().muted_foreground;
        let hover = cx.theme().secondary;
        for (index, row) in rows.into_iter().enumerate() {
            let interactive = row.copy.is_some() || !matches!(row.action, RowAction::None);
            let copy = row.copy.clone();
            let action = row.action.clone();
            let owner_name = user.display_name();
            let base = div()
                .id(row.id)
                .flex()
                .flex_col()
                .px_3()
                .py_2()
                .when(index > 0, |this| {
                    this.border_t_1().border_color(cx.theme().border)
                })
                .when(interactive, |this| {
                    this.cursor_pointer()
                        .role(gpui_kit::Role::Button)
                        .aria_label(format!("{}: {}", row.label, row.value.replace('\n', ": ")))
                        .tab_index(0)
                        .hover(move |style| style.bg(hover))
                })
                .child(if row.id == "info-bio" {
                    // The bio takes part in text selection, so the menu can
                    // copy and translate just the selected part.
                    div()
                        .text_sm()
                        .child(crate::ui::message_text::rich_text_line(
                            &row.value,
                            &[],
                            (user_id, 0),
                            false,
                            &self.message_ui.spoiler_revealed,
                            gpui_kit::px(14.0),
                            &std::collections::HashMap::new(),
                            cx,
                        ))
                        .into_any_element()
                } else {
                    crate::ui::bidi_line::aligned_block(row.value.clone())
                        .text_sm()
                        .when(row.hint, |this| this.text_color(muted))
                        .into_any_element()
                })
                .child(div().text_xs().text_color(muted).child(row.label.clone()))
                .when(qr_row == Some(row.id), |this| {
                    this.relative().child(
                        div()
                            .absolute()
                            .right_2()
                            .top_0()
                            .bottom_0()
                            .flex()
                            .items_center()
                            .debug_selector(|| "info-qr".into())
                            .child(crate::ui::peer_qr::qr_row_button(
                                "info-qr",
                                cx.listener(move |this, _, _, cx| {
                                    this.open_peer_qr_box(
                                        quill::peer_qr::state::QrPeer::User(user_id),
                                        cx,
                                    );
                                }),
                            )),
                    )
                });
            let tapped = base.on_click(cx.listener(move |this, _, window, cx| match &action {
                RowAction::None => {
                    if let Some(copy) = &copy {
                        this.copy_profile_text(&copy.text, copy.toast, cx);
                    }
                }
                RowAction::EditBirthday => this.open_birthday_dialog(window, cx),
                RowAction::PickPersonalChannel => this.open_personal_channel_dialog(cx),
                RowAction::PickMainTab => this.open_main_tab_dialog(cx),
                RowAction::OpenChat(chat_id) => {
                    this.dismiss_profile_modal();
                    this.select_listed_chat(*chat_id, window, cx);
                }
                RowAction::EditNote(user_id) => this.open_edit_contact_dialog(*user_id, window, cx),
                RowAction::Collectible { entity, copy_text } => this.open_collectible_info(
                    entity,
                    &owner_name,
                    Some(ChatId(user_id)),
                    copy_text.clone(),
                    cx,
                ),
            }));
            let extra_copy = row.extra_copy.clone();
            let bio = row.id == "info-bio";
            let fragment_note = fragment_url.clone().filter(|_| row.id == "info-phone");
            let element = match row.copy {
                Some(copy) => {
                    let owner = cx.entity().downgrade();
                    tapped
                        .context_menu(move |menu, window, cx| {
                            if bio {
                                return about_text::about_text_menu(
                                    menu,
                                    &owner,
                                    ChatId(user_id),
                                    &copy.text,
                                    copy.toast,
                                    window,
                                    cx,
                                );
                            }
                            let menu =
                                copy_menu(menu, &owner, std::iter::once(&copy).chain(&extra_copy));
                            match fragment_note.clone() {
                                Some(url) => crate::ui::profile_business::fragment_note_menu(
                                    menu,
                                    muted,
                                    url,
                                    owner.clone(),
                                ),
                                None => menu,
                            }
                        })
                        .into_any_element()
                }
                None => tapped.into_any_element(),
            };
            card = card.child(element);
        }
        for element in business_rows {
            card = card.child(element);
        }
        Some(card.into_any_element())
    }
}
