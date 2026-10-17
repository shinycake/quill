//! The Media tab grid in a real window (needs the `demo-capture` feature for
//! gpui-kit's test support): tiles render, a click opens the viewer over the
//! tab, and a long tab builds only the rows on screen.
use crate::ui::app::QuillApp;
use crate::ui::demos::shared_media::photo_message;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, px, size};
use quill::state::SharedMediaTab;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(1400.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-shared-media")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("app view");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

/// Fill the Media tab with `count` photos, newest first (ids 1000 down).
fn show_photos(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, count: i64) {
    let messages = (0..count)
        .map(|n| photo_message(1000 + count - n, n / 4, ""))
        .collect();
    app.update_in(vcx, |app, _, cx| {
        app.demo_fill_shared_media(SharedMediaTab::Media, messages, cx);
    });
    vcx.update(|_, cx| cx.set_reduce_motion(true));
    vcx.run_until_parked();
}

#[gpui_kit::test]
fn tiles_render_and_a_click_opens_the_viewer(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    show_photos(&app, vcx, 4);
    let bounds = vcx
        .debug_bounds("shared-media-tile-1004")
        .expect("the newest tile is on screen");
    assert!(vcx.debug_bounds("shared-media-tile-1001").is_some());
    vcx.simulate_click(bounds.center(), Modifiers::default());
    vcx.run_until_parked();
    let opened = app.read_with(vcx, |app, _| {
        app.viewer.state.current().map(|item| item.message_id.0)
    });
    assert_eq!(opened, Some(1004));
}

#[gpui_kit::test]
fn a_long_tab_builds_only_the_rows_on_screen(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    show_photos(&app, vcx, 400);
    assert!(vcx.debug_bounds("shared-media-tile-1400").is_some());
    // The oldest tile is hundreds of rows below the fold.
    assert!(vcx.debug_bounds("shared-media-tile-1001").is_none());
}
