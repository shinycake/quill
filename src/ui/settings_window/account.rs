//! The Account page (tdesktop "My Account"): your photo and name on top,
//! then a row per profile field with its current value, your accounts,
//! contacts, and Log out at the end.
#![deny(clippy::indexing_slicing)]

use super::QuillApp;
use super::rows::{danger_row, group, link_row, switch_row};
use crate::ui::navigation::NavigationAction as Nav;
use gpui_kit::assets::IconName;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::peer_color::PeerColorTab;

/// What the profile rows show, read once per render.
#[derive(Default)]
struct Profile {
    name: String,
    phone: String,
    username: String,
    bio: String,
    birthday: Option<String>,
    channel: Option<String>,
    photo: Option<std::path::PathBuf>,
}

impl QuillApp {
    fn account_profile(&self) -> Option<Profile> {
        let session = self.session()?;
        let me = session.my_user_id?;
        let user = session.user(me)?;
        let info = session.user_full_info(me);
        let channel = info
            .map_or(0, |i| i.extras.personal_chat_id)
            .ne(&0)
            .then(|| info.and_then(|i| session.chats.get(&i.extras.personal_chat_id)))
            .flatten()
            .map(|chat| chat.title.clone());
        Some(Profile {
            name: user.display_name(),
            phone: if user.phone_number.is_empty() {
                String::new()
            } else {
                crate::ui::group_panels::format_phone(&user.phone_number)
            },
            username: user.editable_username.clone(),
            bio: info.map(|i| i.bio.clone()).unwrap_or_default(),
            birthday: info
                .and_then(|i| i.extras.birthdate)
                .map(crate::ui::group_panels::format_birthday),
            channel,
            photo: session.user_photo_path(me).and_then(|path| {
                quill::local_path::sandboxed_display_path(path, &self.media_display_roots())
            }),
        })
    }

    pub(super) fn account_page(&mut self, cx: &mut Context<Self>) -> Div {
        let profile = self.account_profile().unwrap_or_default();
        let add = || SharedString::from(tr(keys::SETTINGS_CHANNEL_ADD));
        let info_rows = vec![
            link_row(
                "settings-row-name",
                tr(keys::SETTINGS_NAME_LABEL),
                Some(profile.name.clone().into()),
                Self::edit_profile_listener(cx),
                cx,
            ),
            link_row(
                "settings-row-bio",
                tr(keys::BIO_PLACEHOLDER),
                Some(if profile.bio.is_empty() {
                    add()
                } else {
                    profile.bio.replace('\n', " ").into()
                }),
                Self::edit_profile_listener(cx),
                cx,
            ),
            link_row(
                "settings-row-username",
                tr(keys::SETTINGS_USERNAME_LABEL),
                Some(if profile.username.is_empty() {
                    tr(keys::SETTINGS_USERNAME_ADD).into()
                } else {
                    format!("@{}", profile.username).into()
                }),
                Self::edit_profile_listener(cx),
                cx,
            ),
            link_row(
                "settings-row-phone",
                tr(keys::SETTINGS_PHONE_LABEL),
                Some(profile.phone.clone().into()),
                cx.listener(|this, _, _, cx| this.open_change_phone_dialog(cx)),
                cx,
            ),
            link_row(
                "settings-row-birthday",
                tr(keys::SETTINGS_BIRTHDAY_LABEL),
                Some(profile.birthday.clone().map_or_else(add, Into::into)),
                cx.listener(|this, _, window, cx| this.open_birthday_dialog(window, cx)),
                cx,
            ),
            link_row(
                "settings-row-personal-channel",
                tr(keys::SETTINGS_CHANNEL_LABEL),
                Some(profile.channel.clone().map_or_else(add, Into::into)),
                cx.listener(|this, _, _, cx| this.open_personal_channel_dialog(cx)),
                cx,
            ),
            link_row(
                "settings-row-name-color",
                tr(keys::SETTINGS_THEME_NAME_COLOR),
                None,
                cx.listener(|this, _, _, cx| {
                    this.open_peer_color_dialog(PeerColorTab::Profile, cx)
                }),
                cx,
            ),
        ];
        div()
            .flex()
            .flex_col()
            .gap_6()
            .w_full()
            .child(self.account_header(&profile, cx))
            .children(self.settings_phone_suggestion(cx))
            .child(group(None, info_rows))
            .child(group(
                Some(tr(keys::Q_SETTINGS_ACCOUNT_SEC_ACCOUNTS)),
                [self.account_nav_row(Nav::Accounts, tr(keys::Q_NAVIGATION_ACCOUNTS), cx)],
            ))
            .child(self.account_contacts(cx))
            .children(self.birthday_contacts_block(cx))
            .child(group(
                Some(tr(keys::Q_SETTINGS_ACCOUNT_SEC_SESSION)),
                [
                    self.account_nav_row(
                        Nav::Account,
                        tr(keys::Q_NAVIGATION_ACCOUNT_DELETION_AND_INACTIVITY),
                        cx,
                    ),
                    // Log out ends the Account page (owner, 2026-10-21: here,
                    // not pinned to the sidebar).
                    danger_row(
                        "settings-row-log-out",
                        IconName::LogOut,
                        tr(keys::Q_PREFS_LOG_OUT),
                        cx.listener(|this, _, _, cx| {
                            this.settings.log_out_confirm = true;
                            cx.notify();
                        }),
                        cx,
                    ),
                ],
            ))
    }

    /// Contacts: sync with the servers, import a vCard, delete synced ones.
    fn account_contacts(&self, cx: &mut Context<Self>) -> AnyElement {
        let sync_on = self
            .session()
            .map(|s| s.settings.contact_prefs.sync_enabled)
            .unwrap_or(true);
        let notice = self
            .session()
            .and_then(|s| s.users_state.contacts_notice.clone());
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(group(
                Some(tr(keys::CONTACTS_HEADER)),
                [
                    // Desktop has no address book: the switch keeps the
                    // Contacts tab up to date with the servers.
                    switch_row(
                        "contacts-sync-toggle",
                        tr(keys::Q_PREFS_SYNC_CONTACTS),
                        Some(tr(keys::Q_PREFS_KEEPS_THE_CONTACTS_TAB_UP)),
                        sync_on,
                        cx.listener(|this, &on, _, cx| this.set_contact_sync(on, cx)),
                        cx,
                    ),
                    link_row(
                        "contacts-import-open",
                        tr(keys::Q_PREFS_IMPORT_CONTACTS),
                        None,
                        cx.listener(|this, _, window, cx| {
                            this.open_import_contacts_dialog(window, cx)
                        }),
                        cx,
                    ),
                    danger_row(
                        "contacts-delete-synced",
                        IconName::Delete,
                        tr(keys::Q_PREFS_DELETE_SYNCED_CONTACTS),
                        cx.listener(|this, _, _, cx| {
                            this.open_group_confirm(
                                quill::ids::ChatId(0),
                                crate::ui::GroupConfirmAction::DeleteSyncedContacts,
                                cx,
                            );
                        }),
                        cx,
                    ),
                ],
            ))
            .when_some(notice, |this, notice| {
                this.child(
                    div()
                        .px_1()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(notice),
                )
            })
            .into_any_element()
    }

    fn edit_profile_listener(
        cx: &mut Context<Self>,
    ) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        cx.listener(|this, _, window, cx| this.open_edit_profile_dialog(window, cx))
    }

    fn account_nav_row(
        &self,
        action: Nav,
        title: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        link_row(
            SharedString::from(format!("settings-row-{action:?}")),
            title,
            None,
            cx.listener(move |this, _, window, cx| this.navigate(action, window, cx)),
            cx,
        )
    }

    /// The large photo, name, and phone · @username; clicking edits.
    fn account_header(&self, profile: &Profile, cx: &mut Context<Self>) -> AnyElement {
        let mut detail = profile.phone.clone();
        if !profile.username.is_empty() {
            if !detail.is_empty() {
                detail.push_str(" · ");
            }
            detail.push('@');
            detail.push_str(&profile.username);
        }
        div()
            .id("settings-account-header")
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .w_full()
            .py_2()
            .child(
                div()
                    .id("settings-account-photo")
                    .rounded_full()
                    .cursor_pointer()
                    .on_click(
                        cx.listener(|this, _, window, cx| {
                            this.open_edit_profile_dialog(window, cx)
                        }),
                    )
                    .child(crate::ui::chat_row::chat_avatar(
                        &profile.name,
                        profile.photo.as_deref(),
                        96.,
                    )),
            )
            .child(div().text_xl().font_semibold().child(profile.name.clone()))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(detail),
            )
            .into_any_element()
    }
}
