//! The main window brought back on a Mac, and Quit with no key window
//! (needs the `demo-capture` feature for gpui-kit's test support). Which
//! close hides on which platform: `quill::tray`'s tests. The test platform
//! has no `NSWindow` to hide and drops `on_reopen` callbacks, so whether a
//! window is hidden comes in by hand here.
use super::{note_main_window, raise_main_window_if};
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

#[gpui_kit::test]
fn quill_coming_forward_wants_the_main_window_with_nothing_else_open(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-privacy");
    assert!(app.update(&mut vcx, |app, cx| app.main_window_wanted(cx)));
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
    assert!(!app.update(vcx, |app, cx| app.main_window_wanted(cx)));
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
    let main = vcx.window_handle();
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    assert_eq!(vcx.cx.update(|cx| cx.active_window()), Some(main));
    assert!(!app.update(vcx, |app, cx| app.main_window_wanted(cx)));
    vcx.run_until_parked();
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
