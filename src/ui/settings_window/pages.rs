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
pub(super) fn cards(sections: impl IntoIterator<Item = AnyElement>) -> Option<AnyElement> {
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
    pub(super) fn nav_row(
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
            Page::Notifications => column.child(self.notifications_page(cx)),
            Page::Privacy => column.child(self.privacy_page(cx)),
            Page::Devices => column.child(self.sessions_page(cx)),
            Page::AgentAccess => column.child(self.agent_access_page(cx)),
            Page::Chats => column
                .children(cards([self.media_settings_section(cx).into_any_element()]))
                .children(cards([
                    self.setting_spot(
                        &["chats.send_key"],
                        self.appearance_send_key_section(cx),
                        cx,
                    ),
                    self.appearance_spellcheck_section(cx),
                    self.appearance_suggest_emoji_section(cx),
                ]))
                .child(group(
                    Some(tr(keys::Q_SETTINGS_CHATS_SEC_EMOJI)),
                    [
                        self.setting_spot(
                            &["chats.sticker_suggestions"],
                            self.sticker_suggest_section(cx),
                            cx,
                        ),
                        self.setting_spot(
                            &["chats.archived_stickers"],
                            self.nav_row(
                                Nav::ArchivedStickers,
                                tr(keys::STICKERS_YOU_HAVE),
                                None,
                                cx,
                            ),
                            cx,
                        ),
                    ],
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
                let tray_ready = quill::tray::tray_available();
                let tray_options = self.tray_options(tray_ready);
                let tray =
                    quill::tray::tray_setting_switches(tray_ready, self.appearance.show_tray_icon);
                let autostart = self.general_autostart_section(cx);
                let autostart = self.setting_stand_in(
                    !tray_options.tray_icon,
                    &["system.tray_icon"],
                    autostart,
                    cx,
                );
                let autostart = self.setting_stand_in(
                    !tray_options.taskbar_icon,
                    &["system.taskbar_icon"],
                    autostart,
                    cx,
                );
                let autostart = self.setting_stand_in(
                    !tray_options.monochrome_icon,
                    &["system.monochrome_icon"],
                    autostart,
                    cx,
                );
                let autostart = self.setting_stand_in(
                    !tray.start_in_tray,
                    &["system.start_in_tray"],
                    autostart,
                    cx,
                );
                let autostart = self.setting_stand_in(
                    !tray.run_in_background,
                    &["system.when_closed"],
                    autostart,
                    cx,
                );
                let mut system = vec![autostart, self.general_link_handler_section(cx)];
                system.extend(self.general_send_to_section(cx));
                column
                    .children(cards(system))
                    .children(cards(
                        self.window_behavior_sections(quill::tray::tray_available(), cx),
                    ))
                    .children(cards([self.update_settings_section(cx)]))
            }
            Page::Folders => column.child(self.folders_page(cx)),
            Page::DataStorage => column.child(self.data_storage_page(cx)),
            Page::Connection => column.child(self.proxy_settings_body(cx)),
            Page::Calls => {
                self.ensure_ptt_delay_slider(cx);
                column.child(self.call_settings_section(cx))
            }
            Page::Language => column.child(self.language_page(cx)),
            Page::Shortcuts => column
                .children(cards([self.appearance_keybindings_section(cx)]))
                .child(self.shortcuts_reference(cx)),
            Page::Telegram => column.child(group(None, self.telegram_rows(cx))),
            Page::About => column
                .child(self.about_settings_section(cx))
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
        let has_money = money.is_some();
        let has_grams = grams.is_some();
        let gift = self.session().is_some_and(|s| s.premium_possible());
        let premium_ids: &'static [&'static str] = if gift {
            &["telegram.premium"]
        } else {
            &["telegram.premium", "telegram.send_gift"]
        };
        let stars_ids: &'static [&'static str] = match (has_money, has_grams) {
            (true, true) => &["telegram.stars"],
            (false, true) => &["telegram.stars", "telegram.money"],
            (true, false) => &["telegram.stars", "telegram.grams"],
            (false, false) => &["telegram.stars", "telegram.money", "telegram.grams"],
        };
        let premium = self.nav_row(Nav::Premium, tr(keys::PREMIUM_SUMMARY_TITLE), None, cx);
        let stars = self.nav_row(Nav::Stars, tr(keys::Q_NAVIGATION_STARS), None, cx);
        let my_gifts = self.nav_row(Nav::MyGifts, tr(keys::GIFT_STARS_TABS_MY), None, cx);
        let business = self.nav_row(Nav::Business, tr(keys::BUSINESS_TITLE), None, cx);
        let subscriptions = self.nav_row(
            Nav::Subscriptions,
            tr(keys::Q_NAVIGATION_STAR_SUBSCRIPTIONS),
            None,
            cx,
        );
        let mut rows = vec![
            self.setting_spot(premium_ids, premium, cx),
            self.setting_spot(stars_ids, stars, cx),
        ];
        rows.extend(money.map(|row| self.setting_spot(&["telegram.money"], row, cx)));
        rows.extend(grams.map(|row| self.setting_spot(&["telegram.grams"], row, cx)));
        rows.extend([
            self.setting_spot(&["telegram.my_gifts"], my_gifts, cx),
            self.setting_spot(&["telegram.business"], business, cx),
        ]);
        // tdesktop's "Premium Gifting", where Premium can be bought.
        if gift {
            let row = link_row(
                "settings-row-SendGift",
                tr(keys::SETTINGS_GIFT_PREMIUM),
                None,
                cx.listener(|this, _, window, cx| this.open_gift_recipients(window, cx)),
                cx,
            );
            rows.push(self.setting_spot(&["telegram.send_gift"], row, cx));
        }
        rows.push(self.setting_spot(&["telegram.star_subscriptions"], subscriptions, cx));
        rows
    }
}
