//! Issue #1410: "Proxy settings" on the sign-in screen opened Settings,
//! which the sign-in screen never draws.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext, px, size};

fn sign_in_via_proxy(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("wait-phone-proxy")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the window built the app");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

#[gpui_kit::test]
fn the_strip_link_opens_the_proxy_list_over_sign_in(cx: &mut TestAppContext) {
    let (app, mut vcx) = sign_in_via_proxy(cx);
    vcx.run_until_parked();
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.update(|window, cx| window.click("proxy-strip-settings", cx));
    vcx.run_until_parked();
    vcx.update(|window, cx| window.render_frame(cx));
    app.read_with(&vcx, |app, _| {
        assert!(app.settings.proxy.list_box);
        assert!(!app.settings.open, "no Settings window while signed out");
    });
    assert!(vcx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(vcx.update(|window, _| window.try_find("proxy-add").is_some()));

    vcx.update(|window, cx| window.click("proxy-add", cx));
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| {
        assert!(app.settings.proxy.editor.is_some(), "the edit box opens");
    });
}
