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

/// Fill the Media tab with `count` photos, newest first (ids `base` + count
/// down to `base` + 1).
fn show_photos(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, base: i64, count: i64) {
    let messages = (0..count)
        .map(|n| photo_message(base + count - n, n / 4, ""))
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
    show_photos(&app, vcx, 1000, 4);
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
    show_photos(&app, vcx, 1000, 400);
    assert!(vcx.debug_bounds("shared-media-tile-1400").is_some());
    // The oldest tile is hundreds of rows below the fold.
    assert!(vcx.debug_bounds("shared-media-tile-1001").is_none());
}

#[gpui_kit::test]
fn another_page_starts_at_the_top_and_the_same_page_keeps_its_place(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    show_photos(&app, vcx, 1000, 400);
    app.update_in(vcx, |app, _, cx| {
        app.shared_media_grid
            .scroll_down(SharedMediaTab::Media, 1500.);
        cx.notify();
    });
    vcx.run_until_parked();
    let offset = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| {
            app.shared_media_grid.offset_y(SharedMediaTab::Media)
        })
    };
    assert!(offset(&app, vcx).is_some_and(|y| y < px(0.)), "scrolled");
    // The same page drawn again (a re-render, Load more) stays put.
    app.update_in(vcx, |_, _, cx| cx.notify());
    vcx.run_until_parked();
    assert!(offset(&app, vcx).is_some_and(|y| y < px(0.)), "kept");
    // Another chat's first page starts at the top.
    show_photos(&app, vcx, 5000, 400);
    assert_eq!(offset(&app, vcx), Some(px(0.)));
    assert!(vcx.debug_bounds("shared-media-tile-5400").is_some());
}
