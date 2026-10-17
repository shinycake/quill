//! Groups in common: the search field and the Load more row.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, TestAppContext, VisualTestContext, px, size};

#[gpui_kit::test]
fn a_long_list_has_a_search_field_and_a_load_more_row(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let demo = ScreenshotDemo::named("ready-info-common-groups");
    let handle = cx.open_window(size(px(1200.), px(2400.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
    assert!(vcx.update(|window, _| window.try_find("info-common-groups-search").is_some()));
    assert!(vcx.update(|window, _| window.try_find("info-common-groups-more").is_some()));
}
