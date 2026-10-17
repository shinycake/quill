//! The channel description is a linkified block in the info panel.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, TestAppContext, VisualTestContext, px, size};

#[gpui_kit::test]
fn the_channel_description_is_an_observable_block(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let demo = ScreenshotDemo::named("ready-info-channel");
    let handle = cx.open_window(size(px(1200.), px(1000.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
    assert!(vcx.update(|window, _| window.try_find("info-about").is_some()));
}
