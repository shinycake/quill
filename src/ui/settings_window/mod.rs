//! The settings window: it covers the chat list and the chat while open,
//! with the page list on the left and one page on the right. Pages come
//! from `quill::settings_catalog`; under `NARROW` the list is a screen of
//! its own and a page opens over it with a back arrow.
#![deny(clippy::indexing_slicing)]

mod account;
mod log_out;
mod pages;
mod rows;
mod search;
mod sidebar;

pub(in crate::ui) use search::SettingsSearch;

use super::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::settings_catalog::Page;

/// Below this window width the page list and a page take turns.
const NARROW: f32 = 720.;
/// The readable width of a page's column.
const PAGE_WIDTH: f32 = 640.;

/// A page reached from a row of another page, with a back arrow to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SubPage {
    /// Advanced → Experimental settings.
    Experimental,
    /// About → Ask a Question.
    AskQuestion,
}

impl SubPage {
    fn parent(self) -> Page {
        match self {
            SubPage::Experimental => Page::Advanced,
            SubPage::AskQuestion => Page::About,
        }
    }
}

impl QuillApp {
    /// Open the settings window, on `page` when given (else where it was).
    pub(super) fn open_settings(&mut self, page: Option<Page>) {
        self.settings.open = true;
        self.settings.sub = None;
        if page.is_some() {
            self.settings.page = page;
        }
    }

    pub(super) fn close_settings(&mut self) {
        self.settings.search.close();
        self.settings.open = false;
        self.settings.page = None;
        self.settings.sub = None;
    }

    pub(super) fn show_settings_page(&mut self, page: Page, cx: &mut Context<Self>) {
        self.settings.page = Some(page);
        self.settings.sub = None;
        if page == Page::Privacy {
            // tdesktop asks for the passkeys whenever the screen opens.
            self.request_passkeys();
        }
        cx.notify();
    }

    pub(super) fn show_settings_sub(&mut self, sub: SubPage, cx: &mut Context<Self>) {
        self.settings.page = Some(sub.parent());
        self.settings.sub = Some(sub);
        cx.notify();
    }

    /// One step back: a subpage to its page, a page to the list on a narrow
    /// window, else out of Settings.
    fn settings_back(&mut self, narrow: bool, cx: &mut Context<Self>) {
        if self.settings.sub.take().is_none() {
            if narrow && self.settings.page.is_some() {
                self.settings.page = None;
            } else {
                self.close_settings();
            }
        }
        cx.notify();
    }

    /// The page the window shows: the chosen one, Account by default.
    fn settings_page_shown(&self) -> Page {
        self.settings.page.unwrap_or(Page::Account)
    }

    /// The whole settings window, in place of the chat list and the chat.
    pub(in crate::ui) fn settings_window(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let narrow = f32::from(window.viewport_size().width) < NARROW;
        self.settle_settings_search(window, cx);
        self.settings_search_input(window, cx);
        if !window.has_active_dialog(cx) && window.focused(cx).is_none() {
            window.focus(&self.settings.focus, cx);
        }
        let searching = self.settings_searching(window, cx);
        // A narrow window shows the results in the list's place.
        let list = !narrow || (self.settings.page.is_none() && !searching);
        let page = (!narrow || self.settings.page.is_some()).then(|| self.settings_page_shown());
        div()
            .id("settings-window")
            .debug_selector(|| "settings-window".into())
            .track_focus(&self.settings.focus)
            .flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .bg(cx.theme().background)
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.settings_search_key(event, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape"
                    && this.settings.keybinding_capture.is_none()
                    && !window.has_active_dialog(cx)
                {
                    this.settings_back(narrow, cx);
                    cx.stop_propagation();
                }
            }))
            .when(list, |this| {
                this.child(
                    div()
                        .debug_selector(|| "settings-sidebar".into())
                        .h_full()
                        .when(narrow, |this| this.flex_1())
                        .child(self.settings_sidebar(narrow, searching, cx)),
                )
            })
            .when(searching, |this| {
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .when(narrow, |this| {
                            this.child(div().px_4().pt_4().children(self.settings_search_field(cx)))
                        })
                        .child(self.settings_search_view(cx)),
                )
            })
            .when(!searching, |this| {
                this.when_some(page, |this, page| {
                    this.child(self.settings_page_view(page, narrow, cx))
                })
            })
            .into_any_element()
    }

    /// The right side: a page title and the page in a readable column.
    fn settings_page_view(&mut self, page: Page, narrow: bool, cx: &mut Context<Self>) -> Div {
        let sub = self.settings.sub;
        let back = narrow || sub.is_some();
        let title = match sub {
            Some(sub) => pages::sub_title(sub),
            None => quill::lang::tr(page.title()),
        };
        let body = match sub {
            Some(sub) => self.settings_sub_body(sub, cx),
            None => self.settings_page_body(page, cx),
        };
        div().flex_1().min_w_0().h_full().child(
            div()
                .id(SharedString::from(format!("settings-page-{}", page.id())))
                .debug_selector(move || format!("settings-page-{}", page.id()))
                .size_full()
                .overflow_y_scroll()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_6()
                        .w_full()
                        .max_w(px(PAGE_WIDTH))
                        .mx_auto()
                        .px_6()
                        .py_6()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .when(back, |this| {
                                    this.child(
                                        Button::new("settings-page-back")
                                            .icon(IconName::ArrowLeft)
                                            .ghost()
                                            .accessibility_label(quill::lang::tr(
                                                quill::lang::keys::Q_CHAT_BACK,
                                            ))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.settings_back(narrow, cx);
                                            })),
                                    )
                                })
                                .child(div().text_xl().font_semibold().child(title)),
                        )
                        .child(body),
                ),
        )
    }
}

crate::ui::screenshot_demo::register_demos![
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-settings")
        .setup(|app, _, _| app.demo_settings(Page::Account)),
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-settings-privacy")
        .setup(|app, _, _| app.demo_settings(Page::Privacy)),
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-settings-chats")
        .setup(|app, _, _| app.demo_settings(Page::Chats)),
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-settings-telegram")
        .setup(|app, _, _| app.demo_settings(Page::Telegram)),
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-settings-about")
        .setup(|app, _, _| app.demo_settings(Page::About)),
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-settings-log-out").setup(|app, _, _| {
        app.demo_settings(Page::Account);
        app.settings.log_out_confirm = true;
    }),
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-settings-search").setup(
        |app, window, cx| {
            app.demo_settings(Page::Account);
            app.focus_settings_search(Some("notif"), window, cx);
        }
    ),
];

impl QuillApp {
    /// Screenshot demos: Settings on `page`, signed in as the demo profile.
    fn demo_settings(&mut self, page: Page) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui
                .seq
                .store(session.last_seq, std::sync::atomic::Ordering::SeqCst);
            super::profile::apply_ready_profile_edit(
                session,
                &self.demo_ui.sink,
                &self.demo_ui.seq,
            );
        }
        self.open_settings(Some(page));
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests;
