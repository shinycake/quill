//! The main window brought back on a Mac, and Quit with no key window
//! (needs the `demo-capture` feature for gpui-kit's test support). Which
//! close hides on which platform: `quill::tray`'s tests. The test platform
//! has no `NSWindow` to hide, so whether a window is hidden comes in by
//! hand here.
use super::{MainWindow, main_window_on_activation, note_main_window, raise_main_window_if};
use crate::ui::actions::QuitApp;
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualContext, VisualTestContext, px, size};
use quill::ids::ChatId;

fn new_app(cx: &mut TestAppContext, demo: &'static str) -> (Entity<QuillApp>, VisualTestContext) {
    let root = quill::scratch::process_dir("quill-link-inbox");
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx
            .new(|cx| QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named(demo))));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone();
    let Some(app) = app else {
        panic!("the window builder did not run");
    };
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

#[gpui_kit::test]
fn a_dock_click_raises_a_hidden_main_window_and_leaves_one_that_shows(cx: &mut TestAppContext) {
    let (_app, mut vcx) = new_app(cx, "ready-privacy");
    let vcx = &mut vcx;
    vcx.update(|window, cx| note_main_window(window, cx));
    vcx.deactivate_window();
    vcx.cx.update(|cx| raise_main_window_if(|_| false, cx));
    vcx.run_until_parked();
    assert!(!vcx.update(|window, _| window.is_window_active()));
    vcx.cx.update(|cx| raise_main_window_if(|_| true, cx));
    vcx.run_until_parked();
    assert!(vcx.update(|window, _| window.is_window_active()));
}

/// The main window as a close left it: noted, closed, not active.
fn close_main_window(vcx: &mut VisualTestContext) {
    vcx.update(|window, cx| note_main_window(window, cx));
    vcx.cx
        .update(|cx| cx.global_mut::<MainWindow>().closed_by_user = true);
    vcx.deactivate_window();
}

/// macOS reports Quill active, with the main window hidden.
fn come_forward(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    let view = app.downgrade();
    vcx.cx
        .update(|cx| main_window_on_activation(&view, |_| true, cx));
    vcx.run_until_parked();
}

#[gpui_kit::test]
fn quill_coming_forward_raises_the_closed_main_window(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-privacy");
    let vcx = &mut vcx;
    close_main_window(vcx);
    come_forward(&app, vcx);
    assert!(vcx.update(|window, _| window.is_window_active()));
}

#[gpui_kit::test]
fn a_main_window_the_user_did_not_close_stays_hidden_when_quill_comes_forward(
    cx: &mut TestAppContext,
) {
    let (app, mut vcx) = new_app(cx, "ready-privacy");
    let vcx = &mut vcx;
    vcx.update(|window, cx| note_main_window(window, cx));
    vcx.deactivate_window();
    come_forward(&app, vcx);
    assert!(!vcx.update(|window, _| window.is_window_active()));
}

#[gpui_kit::test]
fn a_chat_window_keeps_the_main_window_closed_when_quill_comes_forward(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    vcx.run_until_parked();
    app.update_in(vcx, |app, window, cx| {
        app.open_chat_window_at(ChatId(11), None, window, cx);
    });
    vcx.run_until_parked();
    close_main_window(vcx);
    come_forward(&app, vcx);
    assert!(!vcx.update(|window, _| window.is_window_active()));
}

#[gpui_kit::test]
fn a_call_takes_the_focus_when_quill_comes_forward(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-video-call");
    let vcx = &mut vcx;
    vcx.run_until_parked();
    app.update(vcx, |app, cx| app.sync_call_window(cx));
    vcx.run_until_parked();
    let Some(call) = app.read_with(vcx, |app, _| app.calls.window) else {
        panic!("the call window did not open");
    };
    close_main_window(vcx);
    come_forward(&app, vcx);
    assert_eq!(vcx.cx.update(|cx| cx.active_window()), Some(call));
}

#[gpui_kit::test]
fn quit_works_with_no_window_open(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-privacy");
    let vcx = &mut vcx;
    let main = vcx.window_handle();
    let weak = app.downgrade();
    vcx.update(|_, cx| crate::ui::os_menus::register_actions(main, weak, cx));
    vcx.update(|window, _| window.remove_window());
    vcx.run_until_parked();
    assert!(vcx.cx.windows().is_empty());
    assert!(vcx.cx.update(|cx| cx.is_action_available(&QuitApp)));
}
