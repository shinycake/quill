//! B10: profile and contact panels — copyable detail rows, edit / share
//! contact, birthday, personal channel, private note, groups in common,
//! similar channels and the profile photo gallery. Behaviour follows
//! tdesktop's `info_profile_actions.cpp`, `edit_contact_box.cpp` and
//! `window_peer_menu.cpp` (see `docs/decisions/codex-profile-panels.md`).

use super::app::QuillApp;
use super::chat_row::chat_avatar;
use super::chat_theme::accent;
use super::dialogs::{BirthdayDialog, EditContactDialog, ProfileDialog};
use super::group_panels::format_phone;
use super::pressable::action_row;
use super::shell::{DialogKind, QuillShell};
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
    /// Open a chat.
    OpenChat(ChatId),
    /// Open the edit-contact box focused on the note.
    EditNote(i64),
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
    label: &'static str,
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
        self.status_note = toast.into();
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
                    menu: "Copy Bio",
                    toast: "Bio copied to clipboard",
                }),
                value: bio,
                label: "Bio",
                action: RowAction::None,
                hint: false,
                extra_copy: None,
            });
        }
        if !user.phone_number.is_empty() {
            rows.push(DetailRow {
                id: "info-phone",
                value: format_phone(&user.phone_number),
                label: "Mobile",
                copy: Some(CopyAction {
                    text: copyable_phone(&user.phone_number),
                    menu: "Copy Phone Number",
                    toast: "Phone number copied to clipboard",
                }),
                action: RowAction::None,
                hint: false,
                extra_copy: None,
            });
        }
        if !user.username.is_empty() {
            rows.push(DetailRow {
                id: "info-username",
                value: format!("@{}", user.username),
                label: "Username",
                copy: Some(CopyAction {
                    text: format!("@{}", user.username),
                    menu: "Copy Username",
                    toast: "Username copied to clipboard",
                }),
                action: RowAction::None,
                hint: false,
                extra_copy: Some(CopyAction {
                    text: profile_link(&user.username),
                    menu: "Copy Link",
                    toast: "Link copied to clipboard",
                }),
            });
            let link = profile_link(&user.username);
            rows.push(DetailRow {
                id: "info-link",
                value: link.trim_start_matches("https://").to_string(),
                label: "Link",
                copy: Some(CopyAction {
                    text: link,
                    menu: "Copy Link",
                    toast: "Link copied to clipboard",
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
                label: "Birthday",
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
                value: "Add your birthday".into(),
                label: "Birthday",
                copy: None,
                action: RowAction::EditBirthday,
                hint: true,
                extra_copy: None,
            }),
            None => {}
        }
        let personal = info.map_or(0, |i| i.extras.personal_chat_id);
        let personal_title = (personal != 0)
            .then(|| session.chats.get(&personal).map(|c| c.title.clone()))
            .flatten();
        match personal_title {
            Some(title) => rows.push(DetailRow {
                id: "info-personal-channel",
                value: title,
                label: "Personal channel",
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
                value: "Add a personal channel".into(),
                label: "Personal channel",
                copy: None,
                action: RowAction::PickPersonalChannel,
                hint: true,
                extra_copy: None,
            }),
            None => {}
        }
        if let Some(note) = info
            .map(|i| i.extras.note.clone())
            .filter(|n| !n.is_empty())
        {
            rows.push(DetailRow {
                id: "info-note",
                copy: Some(CopyAction {
                    text: note.clone(),
                    menu: "Copy Note",
                    toast: "Note copied to clipboard",
                }),
                value: note,
                label: "Note (only visible to you)",
                action: if user.is_contact {
                    RowAction::EditNote(user_id)
                } else {
                    RowAction::None
                },
                hint: false,
                extra_copy: None,
            });
        }
        if rows.is_empty() {
            return None;
        }
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
                        .aria_label(format!("{}: {}", row.label, row.value))
                        .tab_index(0)
                        .hover(move |style| style.bg(hover))
                })
                .child(
                    super::bidi_line::aligned_block(row.value.clone())
                        .text_sm()
                        .when(row.hint, |this| this.text_color(muted)),
                )
                .child(div().text_xs().text_color(muted).child(row.label));
            let tapped = base.on_click(cx.listener(move |this, _, window, cx| match &action {
                RowAction::None => {
                    if let Some(copy) = &copy {
                        this.copy_profile_text(&copy.text, copy.toast, cx);
                    }
                }
                RowAction::EditBirthday => this.open_birthday_dialog(window, cx),
                RowAction::PickPersonalChannel => this.open_personal_channel_dialog(cx),
                RowAction::OpenChat(chat_id) => {
                    this.dismiss_profile_modal();
                    this.select_listed_chat(*chat_id, window, cx);
                }
                RowAction::EditNote(user_id) => this.open_edit_contact_dialog(*user_id, window, cx),
            }));
            let extra_copy = row.extra_copy.clone();
            let element = match row.copy {
                Some(copy) => {
                    let owner = cx.entity().downgrade();
                    tapped
                        .context_menu(move |menu, _, _| {
                            copy_menu(menu, &owner, std::iter::once(&copy).chain(&extra_copy))
                        })
                        .into_any_element()
                }
                None => tapped.into_any_element(),
            };
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
                    "Edit contact",
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
                    "Share this contact",
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
        match session.profile_chat_lists.get(&(kind, key))? {
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
                            "Couldn't load. Retry",
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
            "Similar channels".into(),
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
            live.driver.session.profile_chat_lists.remove(&(kind, key));
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
            self.status_note = "profile photos need a live connection (demo)".into();
            cx.notify();
            return;
        };
        // A failed earlier fetch is retried.
        if matches!(
            live.driver.session.user_profile_photos.get(&user_id),
            Some(ProfilePhotosFetch::Failed(_))
        ) {
            live.driver.session.user_profile_photos.remove(&user_id);
        }
        match live.driver.fetch_user_profile_photos(user_id) {
            Ok(_) => self.pending_profile_gallery = Some(user_id),
            Err(_) => self.status_note = "Couldn't reach Telegram; try again.".into(),
        }
        cx.notify();
    }

    /// Poll-loop hook: open the gallery that was waiting for its list.
    /// Returns `true` when something changed.
    pub(super) fn pump_profile_gallery(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(user_id) = self.pending_profile_gallery else {
            return false;
        };
        let Some(state) = self
            .session()
            .and_then(|s| s.user_profile_photos.get(&user_id))
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
                self.pending_profile_gallery = None;
                self.show_profile_gallery(user_id, cx);
            }
            ProfilePhotosFetch::Loaded { .. } => {
                self.pending_profile_gallery = None;
                self.status_note = "No profile photos".into();
            }
            ProfilePhotosFetch::Failed(reason) => {
                self.pending_profile_gallery = None;
                self.status_note = format!("Couldn't load profile photos: {reason}");
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
        self.media_viewer = MediaViewer::open_profile(items, 0);
        self.viewer_extra.profile_user = Some(user_id);
        self.viewer_extra.profile_personal = personal;
        self.viewer_open_gen += 1;
        self.viewer_note_activity(false, cx);
        self.reset_viewer_item_state(cx);
        cx.notify();
    }

    /// "Set as My Photo" under a suggested profile photo
    /// (`setProfilePhoto` with the suggestion's `chatPhoto.id`).
    pub(super) fn accept_suggested_photo(&mut self, photo_id: i64, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            self.status_note = "profile photo changes need a live connection (demo)".into();
            cx.notify();
            return;
        };
        match live.driver.set_profile_photo_previous(photo_id) {
            Ok(_) => self.status_note = "Profile photo updated".into(),
            Err(_) => self.status_note = "Couldn't reach Telegram; try again.".into(),
        }
        cx.notify();
    }

    /// "Set as main photo" on one of your own earlier photos.
    pub(super) fn set_viewer_photo_as_main(&mut self, cx: &mut Context<Self>) {
        let Some(photo_id) = self.media_viewer.current().map(|item| item.message_id.0) else {
            return;
        };
        let Some(live) = self.live.as_mut() else {
            self.status_note = "profile photo changes need a live connection (demo)".into();
            cx.notify();
            return;
        };
        match live.driver.set_profile_photo_previous(photo_id) {
            Ok(_) => {
                self.status_note = "Main profile photo updated".into();
                self.close_media_viewer(cx);
            }
            Err(_) => self.status_note = "Couldn't reach Telegram; try again.".into(),
        }
        cx.notify();
    }

    /// Open the viewer's "Report" for the photo on screen: the viewer
    /// closes first so the reason list is not hidden behind it.
    pub(super) fn report_viewer_profile_photo(&mut self, cx: &mut Context<Self>) {
        let Some(user_id) = self.viewer_extra.profile_user else {
            return;
        };
        let Some(file_id) = self
            .media_viewer
            .current()
            .map(|item| item.download_file_id.0)
        else {
            return;
        };
        self.close_media_viewer(cx);
        self.profile_dialog = Some(ProfileDialog::ReportPhoto { user_id, file_id });
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
            self.status_note = "Demo mode: reports need a live session.".into();
            cx.notify();
            return;
        };
        match live
            .driver
            .report_profile_photo(user_id, file_id, reason.td_type())
        {
            Ok(_) => self.status_note = "Report sent".into(),
            Err(_) => self.status_note = "Couldn't reach Telegram; try again.".into(),
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
            self.profile_dialog = Some(ProfileDialog::PersonalPhoto {
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
                        this.status_note = "Choose a JPEG, PNG or WebP image.".into();
                        cx.notify();
                        return;
                    }
                    this.profile_dialog = Some(ProfileDialog::PersonalPhoto {
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

    fn submit_personal_photo(
        &mut self,
        user_id: i64,
        mode: PersonalPhotoMode,
        path: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live.as_mut() else {
            self.status_note = "Demo mode: photo changes need a live session.".into();
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
        self.status_note = match sent {
            Ok(_) => mode.done_note().into(),
            Err(_) => "Couldn't reach Telegram; try again.".into(),
        };
        cx.notify();
    }

    // ===================== dialogs =====================

    pub(super) fn close_profile_dialog(&mut self, cx: &mut Context<Self>) {
        self.profile_dialog = None;
        cx.notify();
    }

    /// Edit contact: names, private note and (when the server asks for it)
    /// "Share my phone number".
    pub(super) fn open_edit_contact_dialog(
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
        self.profile_dialog = Some(ProfileDialog::EditContact(dialog));
        cx.notify();
    }

    /// Save the edit-contact box. Only the note changed: `setUserNote`;
    /// otherwise `addContact` (add-or-edit) with the current note so the
    /// note is not cleared.
    fn submit_edit_contact(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(ProfileDialog::EditContact(dialog)) = &self.profile_dialog else {
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
            self.status_note = "Enter a name for the contact.".into();
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
            self.status_note = "Demo mode: contact edits need a live session.".into();
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
                self.status_note = "Contact saved".into();
                true
            }
            Err(_) => {
                self.status_note = "Couldn't reach Telegram; try again.".into();
                cx.notify();
                false
            }
        }
    }

    pub(super) fn open_birthday_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self
            .session()
            .and_then(|s| s.my_user_id.and_then(|me| s.user_full_info(me)))
            .and_then(|i| i.extras.birthdate)
            .map(|b| (b.day, b.month, b.year));
        self.show_birthday_dialog(current, window, cx);
    }

    /// The suggested-birthday card's "View": the same form, filled with
    /// the suggested date (tdesktop opens its edit box the same way).
    pub(super) fn open_suggested_birthday(
        &mut self,
        parts: (u8, u8, Option<i32>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_birthday_dialog(Some(parts), window, cx);
    }

    fn show_birthday_dialog(
        &mut self,
        current: Option<(u8, u8, Option<i32>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dialog = BirthdayDialog::new(window, cx, current);
        dialog
            .day_input
            .update(cx, |input, cx| input.focus(window, cx));
        self.profile_dialog = Some(ProfileDialog::Birthday(dialog));
        cx.notify();
    }

    /// "Choose who can see your birthday": close the form and open the
    /// Privacy editor for the date-of-birth rule (tdesktop links the same
    /// `Privacy::Key::Birthday` box from the birthday row).
    pub(super) fn open_birthday_privacy(&mut self, cx: &mut Context<Self>) {
        self.profile_dialog = None;
        self.open_privacy(cx);
        self.privacy_editor = Some(super::privacy::PrivacyEditorTarget::Rule(
            quill::telegram::requests_privacy::PrivacySettingKey::ShowBirthdate,
        ));
        cx.notify();
    }

    /// `remove` clears the birthday; otherwise the form is validated first
    /// and a bad value stays in the dialog with its message.
    fn submit_birthday(&mut self, remove: bool, cx: &mut Context<Self>) -> bool {
        let parsed = if remove {
            Ok(None)
        } else {
            let Some(ProfileDialog::Birthday(dialog)) = &self.profile_dialog else {
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
                if let Some(ProfileDialog::Birthday(dialog)) = &mut self.profile_dialog {
                    dialog.error = Some(message);
                }
                cx.notify();
                return false;
            }
        };
        let Some(live) = self.live.as_mut() else {
            self.status_note = "Demo mode: birthday changes need a live session.".into();
            cx.notify();
            return true;
        };
        match live.driver.set_birthdate(parts) {
            Ok(_) => {
                self.status_note = if remove {
                    "Birthday removed".into()
                } else {
                    "Birthday saved".into()
                };
                true
            }
            Err(_) => {
                self.status_note = "Couldn't reach Telegram; try again.".into();
                cx.notify();
                false
            }
        }
    }

    pub(super) fn open_personal_channel_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.fetch_suitable_personal_chats();
        }
        self.profile_dialog = Some(ProfileDialog::PersonalChannel);
        cx.notify();
    }

    fn choose_personal_channel(&mut self, chat_id: i64, cx: &mut Context<Self>) -> bool {
        let Some(live) = self.live.as_mut() else {
            self.status_note = "Demo mode: this needs a live session.".into();
            cx.notify();
            return true;
        };
        match live.driver.set_personal_chat(chat_id) {
            Ok(_) => {
                self.status_note = if chat_id == 0 {
                    "Personal channel removed".into()
                } else {
                    "Personal channel saved".into()
                };
                true
            }
            Err(_) => {
                self.status_note = "Couldn't reach Telegram; try again.".into();
                cx.notify();
                false
            }
        }
    }

    pub(super) fn open_share_contact_dialog(&mut self, user_id: i64, cx: &mut Context<Self>) {
        self.profile_dialog = Some(ProfileDialog::ShareContact {
            user_id,
            target: None,
        });
        cx.notify();
    }

    fn send_shared_contact(&mut self, user_id: i64, chat_id: i64, cx: &mut Context<Self>) -> bool {
        let Some(live) = self.live.as_mut() else {
            self.status_note = "Demo mode: sharing needs a live session.".into();
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
                self.status_note = format!("Contact shared with {title}");
                true
            }
            Err(_) => {
                self.status_note = "Couldn't share the contact here.".into();
                cx.notify();
                false
            }
        }
    }

    /// Title, body and footer of the open profile dialog.
    fn profile_dialog_parts(
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
        if matches!(self.profile_dialog, Some(ProfileDialog::AddBot { .. })) {
            return self.add_bot_dialog_parts(cx);
        }
        if matches!(self.profile_dialog, Some(ProfileDialog::ShareGame { .. })) {
            return self.share_game_dialog_parts(cx);
        }
        match self.profile_dialog.as_ref()? {
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
                            "Phone number hidden".to_string()
                        } else {
                            format_phone(&phone)
                        }),
                );
                body = body
                    .child(labeled("First name", &dialog.first_name_input))
                    .child(labeled("Last name", &dialog.last_name_input))
                    .child(labeled("Note", &dialog.note_input));
                if show_share {
                    body = body
                        .child(
                            Checkbox::new("edit-contact-share-phone")
                                .checked(share_phone)
                                .label("Share my phone number")
                                .on_click(cx.listener(|this, &on, window, cx| {
                                    if let Some(ProfileDialog::EditContact(dialog)) =
                                        &mut this.profile_dialog
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
                                "The contact"
                            } else {
                                &name
                            }
                        )));
                }
                let footer =
                    div()
                        .flex()
                        .gap_2()
                        .child(Button::new("edit-contact-save").label("Save").on_click(
                            cx.listener(|this, _, window, cx| {
                                if this.submit_edit_contact(cx) {
                                    this.close_profile_dialog(cx);
                                }
                                this.close_kit_dialog_if_done(DialogKind::ProfilePanel, window, cx);
                            }),
                        ))
                        .child(cancel("edit-contact-cancel", "Cancel", cx));
                Some((
                    "Edit contact".into(),
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
                            .child(div().flex_1().child(labeled("Day", &dialog.day_input)))
                            .child(div().flex_1().child(labeled("Month", &dialog.month_input)))
                            .child(div().flex_1().child(labeled("Year", &dialog.year_input))),
                    )
                    .child(div().text_xs().text_color(muted).child(
                        "The year is optional. Your contacts see your birthday on your profile.",
                    ))
                    .child(
                        div()
                            .id("birthday-privacy-link")
                            .role(gpui_kit::Role::Button)
                            .aria_label("Change who can see your birthday")
                            .tab_index(0)
                            .cursor_pointer()
                            .text_sm()
                            .text_color(accent())
                            .child("Choose who can see your birthday")
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
                            .label("Save")
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.submit_birthday(false, cx) {
                                    this.close_profile_dialog(cx);
                                }
                                this.close_kit_dialog_if_done(DialogKind::ProfilePanel, window, cx);
                            })),
                    )
                    .child(cancel("birthday-cancel", "Cancel", cx));
                if has_birthday {
                    footer = footer.child(
                        Button::new("birthday-remove")
                            .label("Remove birthday")
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
                    "Your birthday".into(),
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
                    .profile_chat_lists
                    .get(&(ProfileChatsKind::SuitablePersonalChats, 0))
                    .cloned();
                let mut body =
                    div().flex().flex_col().gap_1().child(
                        div().text_xs().text_color(muted).pb_1().child(
                            "Show a channel on your profile. Only channels you own are listed.",
                        ),
                    );
                match fetch {
                    None | Some(ProfileChatsFetch::Loading) => {
                        body = body.child(div().text_sm().text_color(muted).child("Loading…"));
                    }
                    Some(ProfileChatsFetch::Failed(_)) => {
                        body = body.child(
                            action_row(
                                "personal-channel-retry",
                                Some(IconName::RotateCw),
                                "Couldn't load channels. Retry",
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
                                .child("You don't have any channels to show yet."),
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
                            .label("Remove personal channel")
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.choose_personal_channel(0, cx) {
                                    this.close_profile_dialog(cx);
                                }
                                this.close_kit_dialog_if_done(DialogKind::ProfilePanel, window, cx);
                            })),
                    );
                }
                footer = footer.child(cancel("personal-channel-cancel", "Close", cx));
                Some((
                    "Personal channel".into(),
                    body.into_any_element(),
                    footer.into_any_element(),
                ))
            }
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
                    let body = div().text_sm().child(format!(
                        "Share {name}'s contact with {}?",
                        chat_title(chat_id)
                    ));
                    let footer = div()
                        .flex()
                        .gap_2()
                        .child(Button::new("share-contact-send").label("Send").on_click(
                            cx.listener(move |this, _, window, cx| {
                                if this.send_shared_contact(user_id, chat_id, cx) {
                                    this.close_profile_dialog(cx);
                                }
                                this.close_kit_dialog_if_done(DialogKind::ProfilePanel, window, cx);
                            }),
                        ))
                        .child(
                            Button::new("share-contact-back")
                                .label("Back")
                                .ghost()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.profile_dialog = Some(ProfileDialog::ShareContact {
                                        user_id,
                                        target: None,
                                    });
                                    window.refresh();
                                    cx.notify();
                                })),
                        );
                    return Some((
                        "Share contact".into(),
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
                    div()
                        .text_xs()
                        .text_color(muted)
                        .pb_1()
                        .child(format!("Choose where to send {name}'s contact.")),
                );
                if chats.is_empty() {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child("No chats loaded yet."),
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
                                this.profile_dialog = Some(ProfileDialog::ShareContact {
                                    user_id,
                                    target: Some(id),
                                });
                                window.refresh();
                                cx.notify();
                            },
                        )),
                    );
                }
                let footer =
                    div()
                        .flex()
                        .gap_2()
                        .child(cancel("share-contact-cancel", "Cancel", cx));
                Some((
                    "Share contact".into(),
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
                    .child(cancel("personal-photo-cancel", "Cancel", cx));
                Some((
                    mode.title().into(),
                    body.into_any_element(),
                    footer.into_any_element(),
                ))
            }
            ProfileDialog::ReportPhoto { user_id, file_id } => {
                let (user_id, file_id) = (*user_id, *file_id);
                let mut body = div().flex().flex_col().gap_1().child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .pb_1()
                        .child("Why are you reporting this photo?"),
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
                                this.submit_photo_report(user_id, file_id, reason, cx);
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
                        .child(cancel("report-photo-cancel", "Cancel", cx));
                Some((
                    "Report".into(),
                    body.into_any_element(),
                    footer.into_any_element(),
                ))
            }
        }
    }

    /// The kit `Dialog` host for every profile panel dialog.
    pub(super) fn build_profile_panel_dialog(
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
                    .title(crate::ui::shell::dialog_title("Profile"))
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
