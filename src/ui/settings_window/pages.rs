//! What each settings page shows. Pages whose controls still live in their
//! own boxes list rows that open those boxes over the settings window, so
//! closing one comes back here.
#![deny(clippy::indexing_slicing)]

use super::rows::{danger_row, group, link_row};
use super::{QuillApp, SubPage};
use crate::ui::navigation::NavigationAction as Nav;
use gpui_kit::assets::IconName;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::settings_catalog::Page;

pub(super) fn sub_title(sub: SubPage) -> &'static str {
    tr(match sub {
        SubPage::Experimental => keys::SETTINGS_EXPERIMENTAL,
        SubPage::AskQuestion => keys::SETTINGS_ASK_QUESTION,
    })
}

impl QuillApp {
    /// A row that runs a navigation action (most open a box).
    fn nav_row(
        &self,
        action: Nav,
        title: &'static str,
        value: Option<SharedString>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        link_row(
            SharedString::from(format!("settings-row-{action:?}")),
            title,
            value,
            cx.listener(move |this, _, window, cx| this.navigate(action, window, cx)),
            cx,
        )
    }

    pub(super) fn settings_page_body(&mut self, page: Page, cx: &mut Context<Self>) -> AnyElement {
        let column = div().flex().flex_col().gap_6().w_full();
        match page {
            Page::Account => column
                .children(self.settings_phone_suggestion(cx))
                .child(group(
                    None,
                    [
                        self.nav_row(Nav::Profile, tr(keys::SETTINGS_INFORMATION), None, cx),
                        self.nav_row(Nav::Accounts, tr(keys::Q_NAVIGATION_ACCOUNTS), None, cx),
                    ],
                ))
                .child(self.contacts_settings_section(cx))
                .child(group(
                    Some(tr(keys::Q_SETTINGS_ACCOUNT_SEC_SESSION)),
                    [self.nav_row(
                        Nav::Account,
                        tr(keys::Q_NAVIGATION_ACCOUNT_DELETION_AND_INACTIVITY),
                        None,
                        cx,
                    )],
                ))
                // Log out ends the Account page (owner, 2026-10-21: here,
                // not pinned to the sidebar).
                .child(group(
                    None,
                    [danger_row(
                        "settings-row-log-out",
                        IconName::LogOut,
                        tr(keys::Q_PREFS_LOG_OUT),
                        cx.listener(|this, _, _, cx| {
                            this.settings.log_out_confirm = true;
                            cx.notify();
                        }),
                        cx,
                    )],
                )),
            Page::Notifications => column.child(group(
                None,
                [self.nav_row(
                    Nav::Notifications,
                    tr(keys::SETTINGS_SECTION_NOTIFY),
                    None,
                    cx,
                )],
            )),
            Page::Privacy => column.child(group(
                None,
                [
                    link_row(
                        "settings-row-privacy-rules",
                        tr(keys::Q_NAVIGATION_PRIVACY_RULES_AND_BLOCKED_USERS),
                        None,
                        cx.listener(|this, _, _, cx| this.open_privacy(cx)),
                        cx,
                    ),
                    self.nav_row(Nav::Passcode, tr(keys::SETTINGS_PASSCODE_TITLE), None, cx),
                ]
                .into_iter()
                .chain(self.passkeys_settings_row(cx))
                .chain([
                    self.nav_row(
                        Nav::TwoFa,
                        tr(keys::SETTINGS_CLOUD_PASSWORD_START_TITLE),
                        None,
                        cx,
                    ),
                    self.nav_row(Nav::Websites, tr(keys::SETTINGS_LOGGED_IN), None, cx),
                ]),
            )),
            Page::Devices => column.child(group(
                None,
                [self.nav_row(Nav::Sessions, tr(keys::Q_NAVIGATION_DEVICES), None, cx)],
            )),
            Page::Chats => column.child(self.media_settings_section(cx)).child(group(
                None,
                [self.nav_row(
                    Nav::ArchivedStickers,
                    tr(keys::Q_NAVIGATION_ARCHIVED_STICKERS),
                    None,
                    cx,
                )],
            )),
            Page::Appearance | Page::Power | Page::System => column.child(group(
                None,
                [self.nav_row(Nav::Appearance, tr(keys::Q_NAVIGATION_APPEARANCE), None, cx)],
            )),
            Page::Folders => column.child(group(
                None,
                [link_row(
                    "settings-row-folders",
                    tr(keys::Q_CHATLIST_MANAGE_CHAT_FOLDERS),
                    None,
                    cx.listener(|this, _, _, cx| this.open_folder_manage(cx)),
                    cx,
                )],
            )),
            Page::DataStorage => column.child(group(
                None,
                [self.nav_row(Nav::Storage, tr(keys::SETTINGS_DATA_STORAGE), None, cx)],
            )),
            Page::Connection => {
                let kind = self.connection_type_row();
                column.child(group(
                    None,
                    [self.nav_row(
                        Nav::Proxy,
                        tr(keys::Q_SETTINGS_PAGE_CONNECTION),
                        Some(kind.into()),
                        cx,
                    )],
                ))
            }
            Page::Calls => {
                self.ensure_ptt_delay_slider(cx);
                column.child(self.call_settings_section(cx))
            }
            Page::Language => column.child(group(
                None,
                [self.nav_row(Nav::Language, tr(keys::SETTINGS_LANGUAGE), None, cx)],
            )),
            Page::Shortcuts => column.child(group(
                None,
                [link_row(
                    "settings-row-shortcuts",
                    tr(keys::Q_SETTINGS_PAGE_SHORTCUTS),
                    None,
                    cx.listener(|this, _, _, cx| {
                        this.settings.shortcuts_open = true;
                        cx.notify();
                    }),
                    cx,
                )],
            )),
            Page::Telegram => column.child(group(None, self.telegram_rows(cx))),
            Page::About => column.child(self.settings_help_footer(cx)),
            Page::Advanced => column.child(self.advanced_settings_page(cx)),
        }
        .into_any_element()
    }

    pub(super) fn settings_sub_body(&mut self, sub: SubPage, cx: &mut Context<Self>) -> AnyElement {
        match sub {
            SubPage::Experimental => self.experimental_settings_page(cx),
            SubPage::AskQuestion => self.ask_question_page(cx),
        }
    }

    /// Premium, Stars, Money, Grams, gifts, Business and subscriptions.
    fn telegram_rows(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let owner = cx.entity().downgrade();
        let money = self.money_settings_row(
            {
                let owner = owner.clone();
                move |_, cx| {
                    let _ = owner.update(cx, |this, cx| this.open_money(cx));
                }
            },
            cx,
        );
        let grams = self.grams_settings_row(
            move |_, cx| {
                let _ = owner.update(cx, |this, cx| this.open_grams(cx));
            },
            cx,
        );
        let mut rows = vec![
            self.nav_row(Nav::Premium, tr(keys::PREMIUM_SUMMARY_TITLE), None, cx),
            self.nav_row(Nav::Stars, tr(keys::Q_NAVIGATION_STARS), None, cx),
        ];
        rows.extend(money);
        rows.extend(grams);
        rows.extend([
            self.nav_row(Nav::MyGifts, tr(keys::GIFT_STARS_TABS_MY), None, cx),
            self.nav_row(Nav::Business, tr(keys::BUSINESS_TITLE), None, cx),
            self.nav_row(
                Nav::Subscriptions,
                tr(keys::Q_NAVIGATION_STAR_SUBSCRIPTIONS),
                None,
                cx,
            ),
        ]);
        rows
    }
}
