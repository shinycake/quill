#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! Hiding the chat list column through GPUI: the shortcut hides it, the
//! chat takes the window, the choice is what the window state saves, and
//! the window's left edge peeks the column over the chat until the pointer
//! leaves it or Escape.

use crate::ui::app::QuillApp;
use crate::ui::keybindings::{SIDEBAR_CHORD, bind_keys};
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use crate::ui::sidebar_reveal::{LEAVE_GRACE, Peek, SLIDE_DURATION};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Bounds, Entity, Modifiers, MouseButton, Pixels, TestAppContext, VisualTestContext,
    point, px, size,
};
use std::time::Duration;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

/// Wait for a slide to land (the motion runs on the wall clock).
fn slide(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    advance(app, vcx, SLIDE_DURATION + Duration::from_millis(60));
    frames(vcx);
}

/// Move the column's clock, which the test holds still.
fn advance(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, by: Duration) {
    app.update(vcx, |app, _| app.frame.sidebar.advance(by));
}

fn open(cx: &mut TestAppContext, width: f32) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(width), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    app.update(&mut vcx, |app, _| app.frame.sidebar.pin());
    frames(&mut vcx);
    (app, vcx)
}

/// The shortcut, pressed while typing in the composer.
fn toggle(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, window, cx| {
        app.composer.update(cx, |input, cx| input.focus(window, cx));
    });
    vcx.simulate_keystrokes(SIDEBAR_CHORD);
    slide(&app, vcx);
}

fn chat(vcx: &mut VisualTestContext) -> Bounds<Pixels> {
    vcx.debug_bounds("conversation-slot")
        .expect("conversation drawn")
}

fn move_to(vcx: &mut VisualTestContext, x: f32) {
    vcx.simulate_mouse_move(
        point(px(x), px(400.)),
        Option::<MouseButton>::None,
        Modifiers::default(),
    );
    frames(vcx);
}

fn hidden_saved(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.update_in(vcx, |app, window, _| {
        app.window_state(window).sidebar_hidden
    })
}

#[gpui_kit::test]
fn the_shortcut_hides_the_column_and_the_chat_takes_the_window(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, 1100.);
    let vcx = &mut vcx;
    let shown = chat(vcx);
    assert!(shown.origin.x > px(200.), "{shown:?}");
    assert!(!hidden_saved(&app, vcx));

    toggle(&app, vcx);
    let hidden = chat(vcx);
    assert_eq!(hidden.origin.x, px(0.));
    assert_eq!(hidden.size.width, px(1100.));
    assert!(hidden_saved(&app, vcx), "the window state saves it");
    assert!(vcx.debug_bounds("sidebar-reveal-edge").is_some());

    toggle(&app, vcx);
    assert_eq!(chat(vcx), shown);
    assert!(!hidden_saved(&app, vcx));
    assert!(vcx.debug_bounds("sidebar-reveal-edge").is_none());
}

#[gpui_kit::test]
fn the_left_edge_peeks_the_column_over_the_chat(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, 1100.);
    let vcx = &mut vcx;
    let column = chat(vcx).origin.x;
    toggle(&app, vcx);
    app.read_with(vcx, |app, _| {
        assert_eq!(app.frame.sidebar.pointer(), Peek::Off);
    });
    assert!(vcx.debug_bounds("sidebar-peek").is_none());

    move_to(vcx, 2.);
    slide(&app, vcx);
    let peek = vcx.debug_bounds("sidebar-peek").expect("peeking");
    assert_eq!(peek.origin.x, px(0.));
    assert_eq!(peek.size.width, column);
    // Over the chat, not beside it.
    assert_eq!(chat(vcx).origin.x, px(0.));

    // Over the column it stays; away from it, it slides out after the grace.
    move_to(vcx, f32::from(column) / 2.);
    move_to(vcx, f32::from(column) + 300.);
    assert!(vcx.debug_bounds("sidebar-peek").is_some(), "grace");
    app.read_with(vcx, |app, _| {
        assert!(matches!(app.frame.sidebar.pointer(), Peek::Leaving { .. }));
    });
    // The grace is measured on the column's clock, and the redraw after it
    // is a timer on the executor's: both are moved by hand.
    advance(&app, vcx, LEAVE_GRACE);
    vcx.executor()
        .advance_clock(LEAVE_GRACE + Duration::from_millis(20));
    slide(&app, vcx);
    assert!(vcx.debug_bounds("sidebar-peek").is_none());
    assert!(vcx.debug_bounds("sidebar-reveal-edge").is_some());

    // Escape closes it at once.
    move_to(vcx, 2.);
    slide(&app, vcx);
    vcx.simulate_keystrokes("escape");
    slide(&app, vcx);
    assert!(vcx.debug_bounds("sidebar-peek").is_none());
    app.read_with(vcx, |app, _| {
        assert_eq!(app.frame.sidebar.pointer(), Peek::Off);
    });
}

#[gpui_kit::test]
fn one_column_has_no_column_to_hide(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, 520.);
    let vcx = &mut vcx;
    toggle(&app, vcx);
    assert!(!hidden_saved(&app, vcx));
    assert!(vcx.debug_bounds("sidebar-reveal-edge").is_none());
}
