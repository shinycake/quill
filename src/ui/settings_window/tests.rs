//! The settings window in the UI (needs the `demo-capture` feature for
//! gpui-kit's test support): it covers the chats, pages switch from the
//! sidebar, boxes open over it, Esc steps back, Log out (on the Account
//! page) asks first, and a narrow window shows the list and a page in turn.

use super::SubPage;
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::{DialogKind, QuillShell};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use quill::settings_catalog::Page;

fn new_app(
    cx: &mut TestAppContext,
    width: f32,
) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    new_app_sized(cx, width, 800.)
}

fn new_app_sized(
    cx: &mut TestAppContext,
    width: f32,
    height: f32,
) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    let root = quill::scratch::process_dir("quill-settings-window");
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    cx.update(gpui_kit::init);
    cx.update(crate::ui::bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(width), px(height)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().expect("app view");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, shell, vcx)
}

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn shown(vcx: &mut VisualTestContext, selector: &'static str) -> bool {
    vcx.debug_bounds(selector).is_some()
}

fn open_dialog(shell: &Entity<QuillShell>, vcx: &mut VisualTestContext) -> Option<DialogKind> {
    vcx.run_until_parked();
    vcx.executor()
        .advance_clock(std::time::Duration::from_millis(300));
    vcx.update(|window, cx| window.render_frame(cx));
    shell.read_with(vcx, |shell, _| shell.open_dialog)
}

#[gpui_kit::test]
fn settings_cover_the_chats_and_esc_brings_them_back(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app(cx, 1100.);
    let vcx = &mut vcx;
    frames(vcx);
    assert!(!shown(vcx, "settings-window"));
    app.update(vcx, |app, cx| {
        app.open_settings(None);
        cx.notify();
    });
    frames(vcx);
    assert!(shown(vcx, "settings-window"));
    // A wide window shows Account until a page is chosen.
    assert!(shown(vcx, "settings-page-account"));
    vcx.simulate_keystrokes("escape");
    frames(vcx);
    app.read_with(vcx, |app, _| assert!(!app.settings.open));
    assert!(!shown(vcx, "settings-window"));
}

#[gpui_kit::test]
fn pages_switch_and_esc_leaves_a_subpage_first(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app(cx, 1100.);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.open_settings(None);
        cx.notify();
    });
    frames(vcx);
    app.update(vcx, |app, cx| app.show_settings_page(Page::Privacy, cx));
    frames(vcx);
    assert!(shown(vcx, "settings-page-privacy"));
    app.update(vcx, |app, cx| {
        app.show_settings_sub(SubPage::AskQuestion, cx)
    });
    frames(vcx);
    assert!(shown(vcx, "settings-page-about"));
    vcx.simulate_keystrokes("escape");
    frames(vcx);
    app.read_with(vcx, |app, _| {
        assert!(app.settings.open);
        assert_eq!(app.settings.page, Some(Page::About));
        assert_eq!(app.settings.sub, None);
    });
}

#[gpui_kit::test]
fn a_box_opens_over_settings_and_closing_it_comes_back(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app(cx, 1100.);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.open_settings(Some(Page::Account));
        cx.notify();
    });
    frames(vcx);
    vcx.update(|window, cx| window.click("settings-row-Accounts", cx));
    assert_eq!(open_dialog(&shell, vcx), Some(DialogKind::Accounts));
    app.update(vcx, |app, cx| {
        app.account.accounts.open = false;
        cx.notify();
    });
    assert_eq!(open_dialog(&shell, vcx), None);
    app.read_with(vcx, |app, _| assert!(app.settings.open));
    assert!(shown(vcx, "settings-page-account"));
}

#[gpui_kit::test]
fn log_out_asks_before_signing_out(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app_sized(cx, 1100., 1600.);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.demo_settings(Page::Account);
        cx.notify();
    });
    frames(vcx);
    vcx.update(|window, cx| window.click("settings-row-log-out", cx));
    assert_eq!(open_dialog(&shell, vcx), Some(DialogKind::LogOut));
    vcx.update(|window, cx| window.click("log-out-cancel", cx));
    assert_eq!(open_dialog(&shell, vcx), None);
    app.read_with(vcx, |app, _| {
        assert!(app.settings.open);
        assert!(!app.settings.log_out_confirm);
    });
}

#[gpui_kit::test]
fn opening_a_chat_closes_settings(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app(cx, 1100.);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.open_settings(Some(Page::Calls));
        cx.notify();
    });
    frames(vcx);
    let chat = app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|s| s.chats.keys().min().copied())
            .map(ChatId)
            .expect("a demo chat")
    });
    app.update_in(vcx, |app, window, cx| {
        app.select_listed_chat(chat, window, cx)
    });
    frames(vcx);
    app.read_with(vcx, |app, _| assert!(!app.settings.open));
    assert!(!shown(vcx, "settings-window"));
}

#[gpui_kit::test]
fn a_narrow_window_shows_the_list_then_a_page_with_a_way_back(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app(cx, 600.);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.open_settings(None);
        cx.notify();
    });
    frames(vcx);
    assert!(shown(vcx, "settings-sidebar"));
    assert!(!shown(vcx, "settings-page-account"));
    app.update(vcx, |app, cx| app.show_settings_page(Page::Calls, cx));
    frames(vcx);
    assert!(!shown(vcx, "settings-sidebar"));
    assert!(shown(vcx, "settings-page-calls"));
    vcx.update(|window, cx| window.click("settings-page-back", cx));
    frames(vcx);
    assert!(shown(vcx, "settings-sidebar"));
    app.read_with(vcx, |app, _| {
        assert!(app.settings.open);
        assert_eq!(app.settings.page, None);
    });
}

fn query(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> String {
    app.read_with(vcx, |app, _| app.settings.search.query.clone())
}

#[gpui_kit::test]
fn typing_searches_and_enter_opens_the_best_match(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app(cx, 1100.);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.open_settings(None);
        cx.notify();
    });
    frames(vcx);
    vcx.update(|window, cx| {
        let focus = app.read(cx).settings.focus.clone();
        window.focus(&focus, cx);
    });
    vcx.simulate_keystrokes("p");
    vcx.simulate_input("roxy");
    frames(vcx);
    assert_eq!(query(&app, vcx), "proxy");
    assert!(shown(vcx, "settings-search-results"));
    assert!(!shown(vcx, "settings-page-account"));
    vcx.simulate_keystrokes("enter");
    frames(vcx);
    assert_eq!(query(&app, vcx), "");
    assert!(shown(vcx, "settings-page-connection"));
    app.read_with(vcx, |app, _| {
        let recent = app.settings.search.recent_ids();
        assert!(
            recent
                .first()
                .is_some_and(|id| id.starts_with("connection")),
            "{recent:?}"
        );
    });
}

#[gpui_kit::test]
fn esc_clears_the_search_before_leaving_settings(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app(cx, 1100.);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.open_settings(Some(Page::Calls));
        cx.notify();
    });
    frames(vcx);
    vcx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-f"
    } else {
        "ctrl-f"
    });
    vcx.simulate_input("notif");
    frames(vcx);
    assert!(shown(vcx, "settings-search-results"));
    vcx.simulate_keystrokes("down up down");
    app.read_with(vcx, |app, _| assert_eq!(app.settings.search.selected(), 1));
    vcx.simulate_keystrokes("escape");
    frames(vcx);
    assert_eq!(query(&app, vcx), "");
    assert!(shown(vcx, "settings-page-calls"));
    app.read_with(vcx, |app, _| assert!(app.settings.open));
    vcx.simulate_keystrokes("escape");
    frames(vcx);
    app.read_with(vcx, |app, _| assert!(!app.settings.open));
}

#[gpui_kit::test]
fn a_narrow_window_shows_results_in_place_of_the_list(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app(cx, 600.);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.open_settings(None);
        cx.notify();
    });
    frames(vcx);
    vcx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-f"
    } else {
        "ctrl-f"
    });
    vcx.simulate_input("language");
    frames(vcx);
    assert!(shown(vcx, "settings-search-results"));
    assert!(!shown(vcx, "settings-sidebar"));
    vcx.simulate_keystrokes("enter");
    frames(vcx);
    assert!(shown(vcx, "settings-page-language"));
}

#[gpui_kit::test]
fn account_rows_open_their_editors(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app_sized(cx, 1100., 1600.);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.demo_settings(Page::Account);
        cx.notify();
    });
    frames(vcx);
    vcx.update(|window, cx| window.click("settings-row-name", cx));
    assert_eq!(open_dialog(&shell, vcx), Some(DialogKind::EditProfile));
    app.update(vcx, |app, cx| app.close_edit_profile_dialog(cx));
    assert_eq!(open_dialog(&shell, vcx), None);
    vcx.update(|window, cx| window.click("settings-row-name-color", cx));
    assert!(open_dialog(&shell, vcx).is_some());
    app.read_with(vcx, |app, _| assert!(app.settings.open));
}

#[gpui_kit::test]
fn notifications_live_on_their_page_and_reset_asks_first(cx: &mut TestAppContext) {
    let (app, shell, mut vcx) = new_app_sized(cx, 1100., 3000.);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.demo_settings(Page::Account);
        cx.notify();
    });
    frames(vcx);
    app.update(vcx, |app, cx| {
        app.show_settings_page(Page::Notifications, cx)
    });
    frames(vcx);
    assert!(shown(vcx, "settings-page-notifications"));
    assert_eq!(open_dialog(&shell, vcx), None, "no box opens over the page");

    vcx.update(|window, cx| window.click("settings-row-reset-notifications", cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.notify.notifications_confirm.is_some()));

    // Leaving the page drops the pending reset.
    app.update(vcx, |app, cx| app.show_settings_page(Page::Account, cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.notify.notifications_confirm.is_none()));
}

#[gpui_kit::test]
fn privacy_rules_open_over_the_page_and_esc_closes_them_first(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app_sized(cx, 1100., 2400.);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.demo_settings(Page::Account);
        app.show_settings_page(Page::Privacy, cx);
    });
    frames(vcx);
    assert!(shown(vcx, "settings-page-privacy"));
    vcx.update(|window, cx| window.click("privacy-rule-userPrivacySettingShowStatus", cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.privacy.editor.is_some()));

    vcx.simulate_keystrokes("escape");
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.privacy.editor.is_none()));
    assert!(shown(vcx, "settings-page-privacy"), "the page stays");
}

#[gpui_kit::test]
fn data_storage_editor_opens_in_the_page_and_leaving_drops_it(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app_sized(cx, 1100., 2400.);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        app.demo_ready_storage_usage(window, cx)
    });
    frames(vcx);
    assert!(shown(vcx, "settings-page-data"));
    app.update(vcx, |app, cx| {
        let network = quill::data_settings::NetworkKind::WiFi;
        let draft = app
            .session()
            .map(|s| *s.settings.data_storage.for_network(network));
        app.settings.data_storage_editor = draft.map(|draft| (network, draft));
        cx.notify();
    });
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.settings.data_storage_editor.is_some()));
    vcx.update(|window, cx| window.click("data-storage-back", cx));
    frames(vcx);
    assert!(
        app.read_with(vcx, |app, _| app.settings.data_storage_editor.is_none()),
        "back returns to the overview"
    );

    app.update(vcx, |app, _| {
        let network = quill::data_settings::NetworkKind::WiFi;
        let draft = app
            .session()
            .map(|s| *s.settings.data_storage.for_network(network));
        app.settings.data_storage_editor = draft.map(|draft| (network, draft));
    });

    app.update(vcx, |app, cx| app.show_settings_page(Page::Connection, cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.settings.data_storage_editor.is_none()));
    assert!(shown(vcx, "settings-page-connection"));
}

#[gpui_kit::test]
fn devices_page_esc_leaves_session_details_before_the_page(cx: &mut TestAppContext) {
    let (app, _, mut vcx) = new_app_sized(cx, 1100., 2400.);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| app.demo_ready_sessions(window, cx));
    frames(vcx);
    assert!(shown(vcx, "settings-page-devices"));
    app.update(vcx, |app, cx| {
        let first = app
            .session()
            .and_then(|s| s.settings.sessions.as_ref())
            .and_then(|list| list.first().map(|s| s.id));
        app.privacy.extra.session_details = first;
        cx.notify();
    });
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.privacy.extra.session_details.is_some()));

    vcx.simulate_keystrokes("escape");
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.privacy.extra.session_details.is_none()));
    assert!(shown(vcx, "settings-page-devices"), "the page stays");
}
