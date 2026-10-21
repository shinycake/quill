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
    let root = quill::scratch::process_dir("quill-settings-window");
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(width), px(800.)), move |window, cx| {
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
    std::thread::sleep(std::time::Duration::from_millis(300));
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
    let (app, shell, mut vcx) = new_app(cx, 1100.);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.open_settings(Some(Page::Account));
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
