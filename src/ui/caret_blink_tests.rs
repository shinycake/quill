#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! The compose field's caret blinks in an active window only (the kit's
//! input). An inactive window doesn't draw the caret, but each blink there
//! redrew the window, twice a second for as long as it stayed in the
//! background, so the window never went idle and kept its spare drawables.

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

fn open(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    vcx.update(|window, cx| window.render_frame(cx));
    (app, vcx)
}

fn focus_composer(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, window, cx| {
        app.composer
            .update(cx, |composer, cx| composer.focus(window, cx));
    });
    vcx.run_until_parked();
}

/// How many times the compose field asks to be redrawn over three seconds
/// of the test clock.
fn redraws(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> usize {
    let count = Rc::new(Cell::new(0));
    let composer = app.read_with(vcx, |app, _| app.composer.clone());
    let _watch = vcx.update(|_, cx| {
        let count = count.clone();
        cx.observe(&composer, move |_, _| count.set(count.get() + 1))
    });
    for _ in 0..6 {
        vcx.executor().advance_clock(Duration::from_millis(500));
        vcx.run_until_parked();
    }
    count.get()
}

/// A field focused while the window is inactive starts blinking only once
/// the window is activated.
#[gpui_kit::test]
fn a_caret_focused_in_an_inactive_window_waits_for_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    vcx.deactivate_window();
    focus_composer(&app, vcx);
    assert_eq!(redraws(&app, vcx), 0, "no blink while inactive");

    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    assert!(redraws(&app, vcx) >= 5, "it blinks once active");
}

/// A field focused in an active window blinks, and deactivating the window
/// stops it (the input's blur handler); the field stays focused.
#[gpui_kit::test]
fn the_caret_stops_when_its_window_deactivates(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    focus_composer(&app, vcx);
    assert!(redraws(&app, vcx) >= 5, "it blinks while active");

    vcx.deactivate_window();
    assert_eq!(redraws(&app, vcx), 0, "no blink in the background");
}
