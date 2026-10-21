//! The What's New window opens from the settings sidebar in a window of its
//! own, comes forward instead of opening twice, and Esc closes it.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    let root = quill::scratch::process_dir("quill-whats-new");
    let _ = std::fs::create_dir_all(&root);
    quill::settings::use_isolated_app_root(root);
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("app view");
    (app, VisualTestContext::from_window(handle.into(), cx))
}

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

#[gpui_kit::test]
fn whats_new_opens_once_from_settings_and_esc_closes_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.demo_release_notes();
        app.open_settings(None);
        cx.notify();
    });
    frames(vcx);
    vcx.update(|window, cx| window.click("settings-whats-new", cx));
    frames(vcx);
    let handle = app
        .read_with(vcx, |app, _| app.whats_new_window())
        .expect("the What's New window opened");
    assert_eq!(vcx.update(|_, cx| cx.windows().len()), 2);

    let mut notes = VisualTestContext::from_window(handle, vcx);
    frames(&mut notes);
    for version in ["2099.1.1", quill::version::APP, "2026.1018.3075"] {
        let selector: &'static str =
            Box::leak(format!("whats-new-release-{version}").into_boxed_str());
        assert!(
            notes.debug_bounds(selector).is_some(),
            "{version} is listed"
        );
    }
    assert!(notes.debug_bounds("whats-new-installed").is_some());

    // Asking again brings the same window forward.
    app.update(vcx, |app, cx| app.open_whats_new(cx));
    frames(vcx);
    assert_eq!(
        app.read_with(vcx, |app, _| app.whats_new_window()),
        Some(handle)
    );
    assert_eq!(vcx.update(|_, cx| cx.windows().len()), 2);

    notes.simulate_keystrokes("escape");
    frames(vcx);
    assert_eq!(app.read_with(vcx, |app, _| app.whats_new_window()), None);
    assert_eq!(vcx.update(|_, cx| cx.windows().len()), 1);
}
