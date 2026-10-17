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

#[gpui_kit::test]
fn a_search_without_matches_says_no_groups_found(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-info-common-groups");
    let handle = cx.open_window(size(px(1200.), px(2400.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
    let Some(app) = slot.borrow().clone() else {
        panic!("the window opened");
    };
    assert!(vcx.update(|window, _| window.try_find("info-common-groups-empty").is_none()));
    app.update_in(&mut vcx, |app, window, cx| {
        if let Some(input) = app.common_groups.search_input() {
            input.update(cx, |input, cx| input.set_value("zzzz", window, cx));
        }
        cx.notify();
    });
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
    let label = vcx.update(|window, _| {
        window
            .try_find("info-common-groups-empty")
            .and_then(|row| row.label().map(str::to_string))
    });
    assert_eq!(label.as_deref(), Some("No groups found"));
}
