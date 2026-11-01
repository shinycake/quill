//! The settings window: it covers the chat list and the chat while open,
//! with the page list on the left and one page on the right. Pages come
//! from `quill::settings_catalog`; under `NARROW` the list is a screen of
//! its own and a page opens over it with a back arrow.
#![deny(clippy::indexing_slicing)]

mod account;
mod agent_access;
mod agent_activity;
mod agent_activity_demo;
mod agent_grants;
mod agent_grants_demo;
mod agent_page;
mod agent_switch;
mod agent_tools;
mod agent_window;
mod log_out;
mod notifications;
mod pages;
mod privacy;
mod rows;
mod search;
mod sidebar;

pub(in crate::ui) use agent_page::AgentPageUi;
pub(in crate::ui) use search::SettingsSearch;

use super::actions::{CancelSearch, OpenChatSearch, OpenSearch};
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
        if quill::agent::ui_driver::guard::refuse_hard_if_injecting(
            quill::agent::ui_driver::guard::HardSurface::Settings,
        ) {
            return;
        }
        self.settings.open = true;
        self.settings.sub = None;
        if let Some(page) = page {
            self.switch_settings_page(page);
        }
        // The next pass sends the grams balance probe, which waits for Settings.
        quill::wake::UPDATES.poke();
    }

    pub(super) fn close_settings(&mut self) {
        if let Some(page) = self.settings.page {
            self.leave_settings_page(page);
        }
        self.settings.search.close();
        self.settings.open = false;
        self.settings.page = None;
        self.settings.sub = None;
    }

    pub(super) fn show_settings_page(&mut self, page: Page, cx: &mut Context<Self>) {
        self.settings.sub = None;
        self.switch_settings_page(page);
        cx.notify();
    }

    /// The one way a page changes: the old page lets go of its drafts and
    /// pickers, the new one asks for what it shows.
    fn switch_settings_page(&mut self, page: Page) {
        if self.settings.page == Some(page) {
            return;
        }
        if let Some(old) = self.settings.page {
            self.leave_settings_page(old);
        }
        self.settings.page = Some(page);
        match page {
            Page::Notifications => self.fetch_notification_settings(),
            // tdesktop asks for the passkeys whenever the screen opens.
            Page::Privacy => {
                self.request_passkeys();
                self.fetch_privacy();
            }
            Page::Devices => self.fetch_sessions(),
            Page::Connection => self.fetch_proxy_list(),
            Page::System => quill::link_handler::read_now(),
            _ => {}
        }
    }

    fn leave_settings_page(&mut self, page: Page) {
        match page {
            Page::Notifications => self.leave_notification_settings(),
            Page::Language => self.leave_language_settings(),
            Page::Devices => self.leave_devices(),
            Page::DataStorage => self.leave_data_storage(),
            Page::AgentAccess => self.leave_agent_page(),
            _ => {}
        }
    }

    pub(super) fn show_settings_sub(&mut self, sub: SubPage, cx: &mut Context<Self>) {
        self.settings.page = Some(sub.parent());
        self.settings.sub = Some(sub);
        cx.notify();
    }

    /// One step back: a subpage to its page, a page to the list on a narrow
    /// window, else out of Settings.
    /// Escape with nothing open over Settings, wherever the focus is.
    pub(in crate::ui) fn settings_escape(&mut self, window: &Window, cx: &mut Context<Self>) {
        let narrow = f32::from(window.viewport_size().width) < NARROW;
        self.settings_back(narrow, cx);
    }

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
    /// Agent access turned off under its page leaves Advanced, where its
    /// switch now is.
    fn settings_page_shown(&self) -> Page {
        match self.settings.page.unwrap_or(Page::Account) {
            page if page.shown(quill::agent::access::enabled()) => page,
            _ => Page::Advanced,
        }
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
        if self.settings.page == Some(Page::Language) {
            self.ensure_language_box(window, cx);
        }
        if self.settings.page == Some(Page::AgentAccess) && quill::agent::access::enabled() {
            self.ensure_agent_page(window, cx);
        }
        if self.settings.page == Some(Page::DataStorage) {
            self.ensure_storage_scroll(window, cx);
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
            // Escape and Find are app-wide bindings: answered here first,
            // so they act on Settings rather than on the chats behind it.
            .on_action(cx.listener(|this, _: &CancelSearch, window, cx| {
                if this.settings.keybinding_capture.is_some() {
                    return;
                }
                if this.search_field_focused(window, cx) {
                    this.clear_settings_search(window, cx);
                } else if !this.dismiss_topmost_layer(window, cx) {
                    this.settings_escape(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &OpenChatSearch, window, cx| {
                this.focus_settings_search(None, window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenSearch, window, cx| {
                this.focus_settings_search(None, window, cx);
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
                .when(page == Page::AgentAccess && sub.is_none(), |this| {
                    this.track_scroll(&self.agent_ui.page.scroll)
                })
                .when(page == Page::DataStorage && sub.is_none(), |this| {
                    this.track_scroll(&self.settings.storage_scroll.page)
                })
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
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-settings-privacy-blocked").setup(
        |app, _, _| {
            app.demo_blocked_users();
            app.demo_settings(Page::Privacy)
        }
    ),
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-settings-chats")
        .setup(|app, _, _| app.demo_settings(Page::Chats)),
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-settings-telegram")
        .setup(|app, _, _| app.demo_settings(Page::Telegram)),
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-settings-notifications")
        .setup(|app, _, _| app.demo_settings(Page::Notifications)),
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-settings-folders").setup(|app, _, _| {
        if let Some(session) = app.demo_session.as_mut() {
            for (id, name, icon) in [
                (1, "Work", "Work"),
                (2, "Family", "Love"),
                (3, "News", "Channels"),
            ] {
                session
                    .chat_list
                    .chat_folders
                    .push(quill::telegram::envelope::ChatFolderInfo {
                        id,
                        name: name.into(),
                        icon_name: icon.into(),
                        color_id: -1,
                        is_shareable: false,
                        has_my_invite_links: false,
                    });
            }
        }
        app.demo_settings(Page::Folders)
    }),
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-settings-appearance")
        .setup(|app, _, _| app.demo_settings(Page::Appearance)),
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-settings-system")
        .setup(|app, _, _| app.demo_settings(Page::System)),
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-settings-power")
        .setup(|app, _, _| app.demo_settings(Page::Power)),
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-settings-shortcuts")
        .setup(|app, _, _| app.demo_settings(Page::Shortcuts)),
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
    /// Screenshot demo: a long blocked-users list with the next page on its
    /// way (`ui/scroll_load.rs`).
    fn demo_blocked_users(&mut self) {
        let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
            self.demo_ui.sink.clone();
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let seq = std::sync::atomic::AtomicU64::new(session.last_seq);
        let names = [
            "Maya", "Leo", "Noa", "Omar", "Ines", "Kai", "Ruth", "Sam", "Tara", "Ugo",
        ];
        let mut ids = Vec::new();
        for (ix, name) in names.iter().enumerate() {
            let id = 9000 + ix as i64;
            let json = format!(
                r#"{{"@type":"updateUser","user":{{"id":{id},"first_name":"{name}","last_name":"Blocked","type":{{"@type":"userTypeRegular"}}}}}}"#
            );
            if let Some(owned) = quill::telegram::client::copy_and_parse(&json, &seq, &sink) {
                session.apply(owned);
            }
            ids.push(id);
        }
        session.settings.blocked_senders = Some(ids);
        session.settings.blocked_total = 120;
        session.settings.blocked_loading = true;
        self.demo_ui.seq.store(
            seq.load(std::sync::atomic::Ordering::SeqCst),
            std::sync::atomic::Ordering::SeqCst,
        );
    }

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
