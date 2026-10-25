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
use quill::lang::{keys, tr, trn};
use quill::local_path::sandboxed_display_path;
use quill::profile_forms::{
    PersonalPhotoMode, PhotoReportReason, is_profile_photo_file, parse_birthday, profile_link,
    unofficial_warning_text,
};
use quill::state::{ProfileChatsFetch, ProfileChatsKind};
use std::cell::RefCell;
use std::rc::Rc;

/// A copy action: the text for the clipboard, the context-menu label and
/// the toast shown afterwards.
#[derive(Clone)]
pub(super) struct CopyAction {
    pub(super) text: String,
    pub(super) menu: &'static str,
    pub(super) toast: &'static str,
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

    /// Share this contact and Edit contact under a person's profile
    /// (tdesktop `ActionsFiller::fillUserActions`: share when the phone is
    /// known, then edit for a contact).
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
        let share = !user.phone_number.is_empty();
        if !share && !user.is_contact {
            return None;
        }
        let mut column = div().flex().flex_col().w_full().gap_0p5();
        if share {
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
        if user.is_contact {
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
                }))
                .test_support(),
            );
        }
        Some(column.into_any_element())
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

    /// "Similar channels" on a channel profile.
    pub(super) fn similar_channels_section(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let list = self.profile_chat_list_section(
            ProfileChatsKind::SimilarChats,
            chat_id.0,
            tr(keys::Q_PEOPLE_SIMILAR_CHANNELS).into(),
            "info-similar-channel",
            "info-similar-channels-retry",
            cx,
        )?;
        Some(match self.similar_unlock(chat_id, cx) {
            Some(unlock) => div()
                .flex()
                .flex_col()
                .w_full()
                .child(list)
                .child(unlock)
                .into_any_element(),
            None => list,
        })
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
pub(in crate::ui) mod common_groups;
pub(in crate::ui) mod contact_photo_rows;
mod details_card;
mod disable_sharing;
mod main_profile_tab_dialog;
mod more_menu;
mod photo_delete;
mod photo_viewer;
mod sharing_request_box;
mod submit_personal_photo;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod about_text_tests;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod action_row_tests;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod common_groups_tests;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod photo_viewer_tests;
