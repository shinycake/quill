//! What each settings page shows. Pages whose controls still live in their
//! own boxes list rows that open those boxes over the settings window, so
//! closing one comes back here.
#![deny(clippy::indexing_slicing)]

use super::rows::{group, link_row};
use super::{QuillApp, SubPage};
use crate::ui::navigation::NavigationAction as Nav;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::settings_catalog::Page;

/// Older settings sections, each a block with its own heading, stacked on
/// one outlined surface.
/// No surface when there is nothing to show (no tray, say).
fn cards(sections: impl IntoIterator<Item = AnyElement>) -> Option<AnyElement> {
    let sections: Vec<AnyElement> = sections.into_iter().collect();
    (!sections.is_empty()).then(|| {
        group(
            None,
            [div()
                .flex()
                .flex_col()
                .gap_5()
                .p_3()
                .children(sections)
                .into_any_element()],
        )
        .into_any_element()
    })
}

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
            Page::Account => self.account_page(cx),
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
            Page::Chats => column
                .children(cards([self.media_settings_section(cx).into_any_element()]))
                .children(cards([
                    self.appearance_send_key_section(cx),
                    self.appearance_spellcheck_section(cx),
                    self.appearance_suggest_emoji_section(cx),
                ]))
                .child(group(
                    None,
                    [self.nav_row(
                        Nav::ArchivedStickers,
                        tr(keys::Q_NAVIGATION_ARCHIVED_STICKERS),
                        None,
                        cx,
                    )],
                )),
            Page::Appearance => {
                self.ensure_wallpapers_loaded(cx);
                if !self.settings.system_accent_probed {
                    self.refresh_system_accent(cx);
                }
                column
                    .children(cards([
                        self.app_themes_section(cx),
                        self.appearance_auto_night_section(cx),
                    ]))
                    .children(cards([self.appearance_peer_color_section(cx)]))
                    .children(cards([
                        self.appearance_wallpaper_section(cx),
                        self.appearance_telegram_wallpapers_section(cx),
                    ]))
                    .children(cards([
                        self.appearance_scale_section(cx),
                        self.appearance_font_section(cx),
                        self.appearance_font_family_section(cx),
                    ]))
                    .children(cards([
                        self.appearance_bubble_section(cx),
                        self.appearance_chat_list_section(cx),
                    ]))
            }
            Page::Power => column.children(cards([self.appearance_power_section(cx)])),
            Page::System => {
                let mut system = vec![
                    self.general_autostart_section(cx),
                    self.general_link_handler_section(cx),
                ];
                system.extend(self.general_send_to_section(cx));
                column
                    .children(cards(system))
                    .children(cards(
                        self.window_behavior_sections(quill::tray::tray_available(), cx),
                    ))
                    .children(cards([self.update_settings_section(cx)]))
            }
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
            Page::Shortcuts => column
                .children(cards([self.appearance_keybindings_section(cx)]))
                .child(group(
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
            Page::About => column
                .children(cards([self.about_settings_section(cx)]))
                .child(self.settings_help_footer(cx)),
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
