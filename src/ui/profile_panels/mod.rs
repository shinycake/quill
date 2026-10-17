//! B10: profile and contact panels — copyable detail rows, edit / share
//! contact, birthday, personal channel, private note, groups in common,
//! similar channels and the profile photo gallery. Behaviour follows
//! tdesktop's `info_profile_actions.cpp`, `edit_contact_box.cpp` and
//! `window_peer_menu.cpp` (see `docs/decisions/codex-profile-panels.md`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_row::chat_avatar;
use super::chat_theme::accent;
use super::dialogs::{BirthdayDialog, EditContactDialog, ProfileDialog};
use super::group_panels::format_phone;
use super::pressable::action_row;
use super::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::local_path::sandboxed_display_path;
use quill::media_viewer::{MediaViewer, profile_photo_items};
use quill::profile_forms::{
    PersonalPhotoMode, PhotoReportReason, is_profile_photo_file, parse_birthday, profile_link,
    unofficial_warning_text,
};
use quill::state::{ProfileChatsFetch, ProfileChatsKind, ProfilePhotosFetch};
use std::cell::RefCell;
use std::rc::Rc;

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

/// A copy action: the text for the clipboard, the context-menu label and
/// the toast shown afterwards.
#[derive(Clone)]
pub(super) struct CopyAction {
    pub(super) text: String,
    pub(super) menu: &'static str,
    pub(super) toast: &'static str,
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

/// A context menu with one item per copy action (tdesktop's profile rows:
/// Copy Username, Copy Link).
pub(super) fn copy_menu<'a>(
    mut menu: PopupMenu,
    owner: &gpui_kit::WeakEntity<QuillApp>,
    actions: impl IntoIterator<Item = &'a CopyAction>,
) -> PopupMenu {
    for copy in actions {
        let owner = owner.clone();
        let copy = copy.clone();
        menu = menu.item(PopupMenuItem::new(copy.menu).on_click(move |_, _, cx| {
            let _ = owner.update(cx, |this, cx| {
                this.copy_profile_text(&copy.text, copy.toast, cx);
            });
        }));
    }
    menu
}

impl QuillApp {
    /// Copy `text` and say so in the status toast (tdesktop shows
    /// "Phone number copied to clipboard" and friends).
    pub(super) fn copy_profile_text(&mut self, text: &str, toast: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
        self.connection.set_note(StatusNote::info(toast));
        cx.notify();
    }

    /// The details card of a user profile: bio, phone, username, link,
    /// birthday, personal channel and the private note. Rows copy their
    /// value on tap and in the right-click menu; the birthday and personal
    /// channel rows of your own profile open their editors instead.
    pub(super) fn profile_details_card(
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
                value: super::group_panels::format_birthday(birthday),
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
                .child(
                    super::bidi_line::aligned_block(row.value.clone())
                        .text_sm()
                        .when(row.hint, |this| this.text_color(muted)),
                )
                .child(div().text_xs().text_color(muted).child(row.label.clone()));
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
            let fragment_note = fragment_url.clone().filter(|_| row.id == "info-phone");
            let element = match row.copy {
                Some(copy) => {
                    let owner = cx.entity().downgrade();
                    tapped
                        .context_menu(move |menu, _, _| {
                            let menu =
                                copy_menu(menu, &owner, std::iter::once(&copy).chain(&extra_copy));
                            match fragment_note.clone() {
                                Some(url) => {
                                    super::profile_business::fragment_note_menu(menu, muted, url)
                                }
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

    /// Edit contact / Share contact rows of a profile (tdesktop's profile
    /// menu: "Edit contact" for contacts, "Share this contact" when the
    /// phone number is known).
    pub(super) fn profile_contact_actions(
        &self,
        user_id: i64,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let user = session.user(user_id)?;
        if session.my_user_id == Some(user_id) || user.is_bot {
            return None;
        }
        let mut column = div().flex().flex_col().w_full().gap_0p5();
        let mut any = false;
        if user.is_contact {
            any = true;
            column = column.child(
                action_row(
                    "info-panel-edit-contact",
                    Some(gpui_kit::assets::IconName::Pencil),
                    tr(keys::INFO_EDIT_CONTACT),
                    false,
                    cx,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_edit_contact_dialog(user_id, window, cx);
                })),
            );
        }
        if user.is_contact {
            any = true;
            column = column.child(
                action_row(
                    "info-panel-set-photo",
                    Some(gpui_kit::assets::IconName::Image),
                    PersonalPhotoMode::Set.title(),
                    false,
                    cx,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.start_personal_photo(user_id, PersonalPhotoMode::Set, cx);
                })),
            );
        }
        if !user.is_bot {
            any = true;
            column = column.child(
                action_row(
                    "info-panel-suggest-photo",
                    Some(gpui_kit::assets::IconName::ImagePlus),
                    PersonalPhotoMode::Suggest.title(),
                    false,
                    cx,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.start_personal_photo(user_id, PersonalPhotoMode::Suggest, cx);
                })),
            );
        }
        if session
            .user_full_info(user_id)
            .is_some_and(|info| info.extras.personal_photo.is_some())
        {
            any = true;
            column = column.child(
                action_row(
                    "info-panel-reset-photo",
                    Some(gpui_kit::assets::IconName::RotateCcw),
                    PersonalPhotoMode::Reset.title(),
                    false,
                    cx,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.start_personal_photo(user_id, PersonalPhotoMode::Reset, cx);
                })),
            );
        }
        if !user.phone_number.is_empty() {
            any = true;
            column = column.child(
                action_row(
                    "info-panel-share-contact",
                    Some(gpui_kit::assets::IconName::Share2),
                    tr(keys::INFO_SHARE_CONTACT),
                    false,
                    cx,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_share_contact_dialog(user_id, cx);
                })),
            );
        }
        any.then(|| column.into_any_element())
    }

    /// tdesktop's divider note under the cover when the user runs an
    /// unofficial client (`AddUnofficialSecurityRiskWarning`).
    pub(super) fn unofficial_client_warning(
        &self,
        user_id: i64,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        if session.my_user_id == Some(user_id)
            || !session
                .user_full_info(user_id)
                .is_some_and(|info| info.extras.uses_unofficial_app)
        {
            return None;
        }
        let first = session.user(user_id)?.first_name.clone();
        let danger = cx.theme().danger;
        Some(
            div()
                .id(("info-unofficial-warning", user_id as u64))
                .flex()
                .items_start()
                .gap_2()
                .w_full()
                .p_3()
                .rounded_md()
                .bg(danger.opacity(0.1))
                .child(
                    gpui_kit::component::Icon::new(IconName::TriangleAlert)
                        .size_4()
                        .text_color(danger)
                        .flex_shrink_0()
                        .mt_0p5(),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .child(unofficial_warning_text(&first)),
                )
                .into_any_element(),
        )
    }

    /// A tappable chat row (avatar and title) for the in-panel lists.
    fn profile_chat_row(
        &self,
        id_prefix: &'static str,
        chat_id: i64,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let chat = session.chats.get(&chat_id)?;
        let title = chat.title.clone();
        let roots = self.media_display_roots();
        let photo = session
            .chat_photo_path(ChatId(chat_id))
            .and_then(|path| sandboxed_display_path(path, &roots));
        Some(
            div()
                .id((id_prefix, chat_id.unsigned_abs()))
                .flex()
                .items_center()
                .gap_3()
                .w_full()
                .px_2()
                .py_1p5()
                .rounded_md()
                .cursor_pointer()
                .role(gpui_kit::Role::Button)
                .aria_label(title.clone())
                .tab_index(0)
                .hover(|style| style.bg(cx.theme().secondary))
                .child(chat_avatar(&title, photo.as_deref(), 32.))
                .child(
                    div()
                        .text_sm()
                        .truncate()
                        .child(super::bidi_line::one_line_plain(title)),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.dismiss_profile_modal();
                    this.select_listed_chat(ChatId(chat_id), window, cx);
                }))
                .into_any_element(),
        )
    }

    /// A titled list of chats from a `profile_chat_lists` entry: the rows
    /// once loaded, a Retry row when the fetch failed, nothing while it is
    /// loading (or empty).
    fn profile_chat_list_section(
        &self,
        kind: ProfileChatsKind,
        key: i64,
        title: String,
        id_prefix: &'static str,
        retry_id: &'static str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session = self.session()?;
        let heading = div()
            .px_2()
            .text_xs()
            .font_semibold()
            .text_color(cx.theme().muted_foreground)
            .child(title);
        match session.users_state.profile_chat_lists.get(&(kind, key))? {
            ProfileChatsFetch::Loaded(ids) => {
                let rows: Vec<AnyElement> = ids
                    .iter()
                    .filter_map(|id| self.profile_chat_row(id_prefix, *id, cx))
                    .collect();
                (!rows.is_empty()).then(|| {
                    div()
                        .flex()
                        .flex_col()
                        .w_full()
                        .gap_0p5()
                        .pt_2()
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .child(heading)
                        .children(rows)
                        .into_any_element()
                })
            }
            ProfileChatsFetch::Failed(_) => Some(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .gap_0p5()
                    .pt_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(heading)
                    .child(
                        action_row(
                            retry_id,
                            Some(IconName::RotateCw),
                            tr(keys::Q_PEOPLE_COULDNT_LOAD_RETRY),
                            false,
                            cx,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.retry_profile_chats(kind, key, cx);
                        })),
                    )
                    .into_any_element(),
            ),
            ProfileChatsFetch::Loading => None,
        }
    }

    /// "N groups in common" with the group rows (tdesktop's common groups
    /// section of the profile).
    pub(super) fn groups_in_common_section(
        &self,
        user_id: i64,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let count = self
            .session()?
            .profile_chat_list(ProfileChatsKind::GroupsInCommon, user_id)
            .len();
        self.profile_chat_list_section(
            ProfileChatsKind::GroupsInCommon,
            user_id,
            format!(
                "{count} group{} in common",
                if count == 1 { "" } else { "s" }
            ),
            "info-common-group",
            "info-common-groups-retry",
            cx,
        )
    }

    /// "Similar channels" on a channel profile.
    pub(super) fn similar_channels_section(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.profile_chat_list_section(
            ProfileChatsKind::SimilarChats,
            chat_id.0,
            tr(keys::Q_PEOPLE_SIMILAR_CHANNELS).into(),
            "info-similar-channel",
            "info-similar-channels-retry",
            cx,
        )
    }

    /// Forget a failed list and ask again.
    pub(super) fn retry_profile_chats(
        &mut self,
        kind: ProfileChatsKind,
        key: i64,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            live.driver
                .session
                .users_state
                .profile_chat_lists
                .remove(&(kind, key));
            let _ = match kind {
                ProfileChatsKind::GroupsInCommon => live.driver.fetch_groups_in_common(key),
                ProfileChatsKind::SimilarChats => live.driver.fetch_similar_chats(ChatId(key)),
                ProfileChatsKind::SuitablePersonalChats => {
                    live.driver.fetch_suitable_personal_chats()
                }
                ProfileChatsKind::SuitableDiscussionChats => {
                    live.driver.fetch_suitable_discussion_chats()
                }
            };
        }
        cx.notify();
    }

    // ===================== profile photos =====================

    /// Open the profile photo gallery of `user_id` in the media viewer.
    /// The list is fetched first when it is not cached; the poll loop
    /// opens the viewer when it lands.
    pub(super) fn open_profile_photos(&mut self, user_id: i64, cx: &mut Context<Self>) {
        let ready = self
            .session()
            .and_then(|s| s.profile_gallery(user_id))
            .is_some_and(|(photos, _)| !photos.is_empty());
        if ready {
            self.show_profile_gallery(user_id, cx);
            return;
        }
        let Some(live) = self.live.as_mut() else {
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_PEOPLE_PROFILE_PHOTOS_NEED_A_LIVE,
            )));
            cx.notify();
            return;
        };
        // A failed earlier fetch is retried.
        if matches!(
            live.driver
                .session
                .users_state
                .user_profile_photos
                .get(&user_id),
            Some(ProfilePhotosFetch::Failed(_))
        ) {
            live.driver
                .session
                .users_state
                .user_profile_photos
                .remove(&user_id);
        }
        match live.driver.fetch_user_profile_photos(user_id) {
            Ok(_) => self.dialogs.pending_profile_gallery = Some(user_id),
            Err(_) => self.connection.set_note(StatusNote::error(tr(
                keys::Q_PEOPLE_COULDNT_REACH_TELEGRAM_TRY_AGAIN,
            ))),
        }
        cx.notify();
    }

    /// Poll-loop hook: open the gallery that was waiting for its list.
    /// Returns `true` when something changed.
    pub(super) fn pump_profile_gallery(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(user_id) = self.dialogs.pending_profile_gallery else {
            return false;
        };
        let Some(state) = self
            .session()
            .and_then(|s| s.users_state.user_profile_photos.get(&user_id))
            .cloned()
        else {
            return false;
        };
        let has_photos = self
            .session()
            .and_then(|s| s.profile_gallery(user_id))
            .is_some_and(|(photos, _)| !photos.is_empty());
        match state {
            ProfilePhotosFetch::Loading => return false,
            ProfilePhotosFetch::Loaded { .. } if has_photos => {
                self.dialogs.pending_profile_gallery = None;
                self.show_profile_gallery(user_id, cx);
            }
            ProfilePhotosFetch::Loaded { .. } => {
                self.dialogs.pending_profile_gallery = None;
                self.connection
                    .set_note(StatusNote::silent(tr(keys::Q_PEOPLE_NO_PROFILE_PHOTOS)));
            }
            ProfilePhotosFetch::Failed(reason) => {
                self.dialogs.pending_profile_gallery = None;
                self.connection.set_note(StatusNote::error(tr_with(
                    keys::Q_PEOPLE_COULDNT_LOAD_PROFILE_PHOTOS,
                    &[("reason", &reason.to_string())],
                )));
            }
        }
        true
    }

    fn show_profile_gallery(&mut self, user_id: i64, cx: &mut Context<Self>) {
        let Some((photos, personal)) = self.session().and_then(|s| s.profile_gallery(user_id))
        else {
            return;
        };
        let items = profile_photo_items(&photos);
        if items.is_empty() {
            return;
        }
        self.viewer.state = MediaViewer::open_profile(items, 0);
        self.viewer.extra.profile_user = Some(user_id);
        self.viewer.extra.profile_personal = personal;
        self.viewer.open_gen += 1;
        self.viewer_note_activity(false, cx);
        self.reset_viewer_item_state(cx);
        cx.notify();
    }

    /// "Set as My Photo" under a suggested profile photo
    /// (`setProfilePhoto` with the suggestion's `chatPhoto.id`).
    pub(super) fn accept_suggested_photo(&mut self, photo_id: i64, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_PEOPLE_PROFILE_PHOTO_CHANGES_NEED_A,
            )));
            cx.notify();
            return;
        };
        match live.driver.set_profile_photo_previous(photo_id) {
            Ok(_) => self
                .connection
                .set_note(StatusNote::silent(tr(keys::Q_PEOPLE_PROFILE_PHOTO_UPDATED))),
            Err(_) => self.connection.set_note(StatusNote::error(tr(
                keys::Q_PEOPLE_COULDNT_REACH_TELEGRAM_TRY_AGAIN,
            ))),
        }
        cx.notify();
    }

    /// "Set as main photo" on one of your own earlier photos.
    pub(super) fn set_viewer_photo_as_main(&mut self, cx: &mut Context<Self>) {
        let Some(photo_id) = self.viewer.state.current().map(|item| item.message_id.0) else {
            return;
        };
        let Some(live) = self.live.as_mut() else {
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_PEOPLE_PROFILE_PHOTO_CHANGES_NEED_A,
            )));
            cx.notify();
            return;
        };
        match live.driver.set_profile_photo_previous(photo_id) {
            Ok(_) => {
                self.connection.set_note(StatusNote::silent(tr(
                    keys::Q_PEOPLE_MAIN_PROFILE_PHOTO_UPDATED,
                )));
                self.close_media_viewer(cx);
            }
            Err(_) => self.connection.set_note(StatusNote::error(tr(
                keys::Q_PEOPLE_COULDNT_REACH_TELEGRAM_TRY_AGAIN,
            ))),
        }
        cx.notify();
    }

    /// Open the viewer's "Report" for the photo on screen: the viewer
    /// closes first so the reason list is not hidden behind it.
    pub(super) fn report_viewer_profile_photo(&mut self, cx: &mut Context<Self>) {
        let Some(user_id) = self.viewer.extra.profile_user else {
            return;
        };
        let Some(file_id) = self
            .viewer
            .state
            .current()
            .map(|item| item.download_file_id.0)
        else {
            return;
        };
        self.close_media_viewer(cx);
        self.dialogs.profile_dialog = Some(ProfileDialog::ReportPhoto { user_id, file_id });
        cx.notify();
    }

    fn submit_photo_report(
        &mut self,
        user_id: i64,
        file_id: i32,
        reason: PhotoReportReason,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live.as_mut() else {
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_PEOPLE_DEMO_MODE_REPORTS_NEED_A,
            )));
            cx.notify();
            return;
        };
        match live
            .driver
            .report_profile_photo(user_id, file_id, reason.td_type())
        {
            Ok(_) => self
                .connection
                .set_note(StatusNote::silent(tr(keys::Q_PEOPLE_REPORT_SENT))),
            Err(_) => self.connection.set_note(StatusNote::error(tr(
                keys::Q_PEOPLE_COULDNT_REACH_TELEGRAM_TRY_AGAIN,
            ))),
        }
        cx.notify();
    }

    /// "Set Profile Photo" and "Suggest Profile Photo": pick an image, then
    /// confirm. "Reset to Original" goes straight to the confirmation.
    pub(super) fn start_personal_photo(
        &mut self,
        user_id: i64,
        mode: PersonalPhotoMode,
        cx: &mut Context<Self>,
    ) {
        if !mode.needs_file() {
            self.dialogs.profile_dialog = Some(ProfileDialog::PersonalPhoto {
                user_id,
                mode,
                path: None,
            });
            cx.notify();
            return;
        }
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(mode.title().into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picker.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, |this, cx| {
                    if !is_profile_photo_file(&path) {
                        this.connection
                            .set_note(StatusNote::silent(tr(keys::Q_PEOPLE_CHOOSE_A_JPEG_PNG_OR)));
                        cx.notify();
                        return;
                    }
                    this.dialogs.profile_dialog = Some(ProfileDialog::PersonalPhoto {
                        user_id,
                        mode,
                        path: Some(path.to_string_lossy().into_owned()),
                    });
                    cx.notify();
                });
            }
        })
        .detach();
    }

    // ===================== dialogs =====================
}

crate::ui::shell::register_dialogs! {
    /// B10: profile and contact panel dialogs.
    ProfilePanel => DialogSpec::new(
        6100,
        |app| app.dialogs.profile_dialog.is_some(),
        QuillApp::build_profile_panel_dialog,
    ),
}

mod about_text;
pub(super) mod action_row;
pub(super) mod bot_rows;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod bot_rows_tests;
mod change_phone_dialog;
mod main_profile_tab_dialog;
mod submit_personal_photo;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod about_text_tests;
