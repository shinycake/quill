//! The settings window's left column: the way back to the chats, your
//! profile, the page list, and the version at the bottom.
#![deny(clippy::indexing_slicing)]

use super::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::sidebar::{Sidebar, SidebarMenu, SidebarMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::settings_catalog::Page;

/// The sidebar icon of each page.
fn page_icon(page: Page) -> IconName {
    match page {
        Page::Account => IconName::CircleUser,
        Page::Notifications => IconName::Bell,
        Page::Privacy => IconName::Lock,
        Page::Devices => IconName::Monitor,
        Page::AgentAccess => IconName::Bot,
        Page::Chats => IconName::MessageCircle,
        Page::Appearance => IconName::Palette,
        Page::Folders => IconName::Folder,
        Page::DataStorage => IconName::ChartPie,
        Page::Connection => IconName::Zap,
        Page::Calls => IconName::Phone,
        Page::Power => IconName::Gauge,
        Page::Language => IconName::Languages,
        Page::Shortcuts => IconName::Keyboard,
        Page::System => IconName::Settings,
        Page::Telegram => IconName::Star,
        Page::About => IconName::Info,
        Page::Advanced => IconName::CodeXml,
    }
}

impl QuillApp {
    pub(super) fn settings_sidebar(
        &self,
        narrow: bool,
        searching: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // No page is beside the list on a narrow window, or while results
        // stand in for it, so none is current.
        let current = (!narrow && !searching).then(|| self.settings_page_shown());
        let owner = cx.entity().downgrade();
        let items = Page::ALL.into_iter().map(|page| {
            let owner = owner.clone();
            SidebarMenuItem::new(tr(page.title()))
                .icon(page_icon(page))
                .active(current == Some(page))
                .on_click(move |_, _, cx| {
                    let _ = owner.update(cx, |this, cx| this.show_settings_page(page, cx));
                })
        });
        Sidebar::new("settings-sidebar")
            .collapsible(false)
            .when(narrow, |this| this.w_full())
            .header(self.settings_sidebar_header(cx))
            .child(SidebarMenu::new().children(items))
            .footer(self.settings_sidebar_footer(cx))
            .into_any_element()
    }

    /// Back to the chats, the window's title and your profile.
    fn settings_sidebar_header(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .w_full()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("settings-close")
                            .icon(IconName::ArrowLeft)
                            .ghost()
                            .small()
                            .tooltip(tr(keys::Q_SETTINGS_BACK_TO_CHATS))
                            .accessibility_label(tr(keys::Q_SETTINGS_BACK_TO_CHATS))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_settings();
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_lg()
                            .font_semibold()
                            .child(tr(keys::MENU_SETTINGS)),
                    ),
            )
            .children(self.settings_search_field(cx))
            .children(self.settings_profile_card(cx))
    }

    /// Your photo, name and phone; opens the Account page.
    fn settings_profile_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let me = session.user(session.my_user_id?)?;
        let name = me.display_name();
        let detail = if me.phone_number.is_empty() {
            String::new()
        } else {
            crate::ui::group_panels::format_phone(&me.phone_number)
        };
        let photo = session.user_photo_path(me.id).and_then(|path| {
            quill::local_path::sandboxed_display_path(path, &self.media_display_roots())
        });
        Some(
            div()
                .id("settings-profile")
                .flex()
                .items_center()
                .gap_3()
                .w_full()
                .px_2()
                .py_2()
                .rounded(cx.theme().radius)
                .cursor_pointer()
                .hover(|this| this.bg(cx.theme().sidebar_accent))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.show_settings_page(Page::Account, cx);
                }))
                .child(crate::ui::chat_row::chat_avatar(
                    &name,
                    photo.as_deref(),
                    40.,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .child(div().text_sm().font_semibold().truncate().child(name))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .truncate()
                                .child(detail),
                        ),
                )
                .into_any_element(),
        )
    }

    /// The version with its release notes.
    fn settings_sidebar_footer(&self, cx: &mut Context<Self>) -> Div {
        div().flex().flex_col().gap_1().w_full().child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .px_2()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(
                    div()
                        .id("settings-version")
                        .truncate()
                        .child(quill::settings_account::current_version_line()),
                )
                .child(
                    Button::new("settings-whats-new")
                        .label(tr(keys::Q_PREFS_WHATS_NEW))
                        .link()
                        .xsmall()
                        .on_click(cx.listener(|this, _, _, cx| this.open_whats_new(cx))),
                ),
        )
    }
}
