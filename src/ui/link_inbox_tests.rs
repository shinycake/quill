//! The main window brought back with no window showing, and Quit with no
//! key window (needs the `demo-capture` feature for gpui-kit's test
//! support). Which close hides on which platform: `quill::tray`'s tests.
use super::{note_main_window, reopen_main_window};
use crate::ui::actions::QuitApp;
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualContext, VisualTestContext, px, size};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    let root = quill::scratch::process_dir("quill-link-inbox");
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-privacy")),
            )
        });
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

/// The handler alone: the test platform drops `on_reopen` callbacks, so
/// the Dock wiring in `main.rs` and a hidden app are not covered.
#[gpui_kit::test]
fn the_reopen_handler_raises_an_inactive_main_window(cx: &mut TestAppContext) {
    let (_app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.update(|window, cx| note_main_window(window, cx));
    vcx.deactivate_window();
    assert!(!vcx.update(|window, _| window.is_window_active()));
    vcx.cx.update(reopen_main_window);
    vcx.run_until_parked();
    assert!(vcx.update(|window, _| window.is_window_active()));
}

#[gpui_kit::test]
fn quit_works_with_no_window_open(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let main = vcx.window_handle();
    let weak = app.downgrade();
    vcx.update(|_, cx| crate::ui::os_menus::register_actions(main, weak, cx));
    vcx.update(|window, _| window.remove_window());
    vcx.run_until_parked();
    assert!(vcx.cx.windows().is_empty());
    assert!(vcx.cx.update(|cx| cx.is_action_available(&QuitApp)));
}
