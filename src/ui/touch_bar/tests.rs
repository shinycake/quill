//! The Touch Bar's install on a real app (not a demo) must not read the
//! app while the app is still being built (a launch crash once).

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, TestAppContext, VisualTestContext, px, size};

#[gpui_kit::test]
fn installing_while_the_app_is_built_does_not_crash(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let demo = ScreenshotDemo::named("ready-stickers");
    let handle = cx.open_window(size(px(1200.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            let app = QuillApp::new_with_demo(window, cx, None, Some(demo));
            // As the app does at launch, inside its own constructor.
            super::install(false, window, cx);
            app
        });
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.run_until_parked();
}
