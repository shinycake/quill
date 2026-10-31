//! Freed heap goes back to the system once Quill goes idle
//! (`docs/decisions/idle-heap-return.md`): one release after a large free;
//! none while the user scrolls or moves the pointer; one, right after a
//! frame, while only an animation draws; none when nothing was freed. Over
//! the demo, on the test clock; nothing reaches Telegram.

use crate::ui::app::QuillApp;
use crate::ui::frame_clock::frames_over as frames;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{
    AppContext, Entity, Modifiers, ScrollDelta, ScrollEvent, ScrollSource, TestAppContext,
    TouchPhase, VisualTestContext, point, px, size,
};
use quill::free_heap::{WORTH_RELEASING, note_freed, releases};
use std::time::Duration;

/// The ready-chats demo in front, settled: what opening it freed went back.
fn open(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let _ = frames(&mut vcx, 5);
    (app, vcx)
}

/// What scrolling through long posts frees (the paragraph cache notes its
/// layouts as it lets them go, `paragraph_cache_tests`), in a frame.
fn free_in_a_frame(vcx: &mut VisualTestContext) {
    note_freed(64 * WORTH_RELEASING);
    draw(vcx);
}

fn draw(vcx: &mut VisualTestContext) {
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    vcx.run_until_parked();
}

#[gpui_kit::test]
fn a_large_free_goes_back_once_quiet(cx: &mut TestAppContext) {
    let (_app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let before = releases();
    free_in_a_frame(vcx);
    let _ = frames(vcx, 1);
    assert_eq!(releases(), before, "not within a second of the free");
    let _ = frames(vcx, 10);
    assert_eq!(releases(), before + 1, "once quiet");
    let _ = frames(vcx, 30);
    assert_eq!(releases(), before + 1, "once only");
}

/// Five seconds of scrolling the history, a wheel step every 50 ms: no
/// release until `AFTER_INPUT` after the last step.
#[gpui_kit::test]
fn a_scroll_in_progress_puts_it_off(cx: &mut TestAppContext) {
    let (_app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let before = releases();
    free_in_a_frame(vcx);
    for n in 0..100 {
        vcx.simulate_event(ScrollEvent {
            position: point(px(800.), px(400.)),
            delta: ScrollDelta::Pixels(point(px(0.), px(if n % 40 < 20 { 30. } else { -30. }))),
            modifiers: Modifiers::default(),
            touch_phase: TouchPhase::Moved,
            source: ScrollSource::Pointer,
        });
        vcx.executor().advance_clock(Duration::from_millis(50));
        draw(vcx);
    }
    assert_eq!(releases(), before, "not while it scrolls");
    let _ = frames(vcx, 3);
    assert_eq!(releases(), before + 1, "once it stopped");
}

/// The pointer moving over the window every 100 ms for five seconds, with
/// nothing drawn: no release until `AFTER_INPUT` after the last move.
#[gpui_kit::test]
fn pointer_input_puts_it_off(cx: &mut TestAppContext) {
    let (_app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let before = releases();
    free_in_a_frame(vcx);
    for n in 0..50 {
        let x = px(600. + (n % 2) as f32);
        vcx.simulate_mouse_move(point(x, px(400.)), None, Modifiers::default());
        vcx.executor().advance_clock(Duration::from_millis(100));
        vcx.run_until_parked();
    }
    assert_eq!(releases(), before, "not while the pointer moves");
    let _ = frames(vcx, 3);
    assert_eq!(releases(), before + 1, "once it stopped");
}

/// Animated content asks the frame clock for a tick every frame, for five
/// seconds, with no input: it gets one release while it plays, made in a
/// step that drew a frame. The timer can't have made it (a frame came
/// within `QUIET` all along), so it ran right after a frame was drawn.
#[gpui_kit::test]
fn an_animation_alone_gets_one_release_after_a_frame(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let before = releases();
    free_in_a_frame(vcx);
    let frame = |vcx: &mut VisualTestContext| vcx.update(|window, _| window.frame_number());
    let mut released_in = Vec::new();
    for _ in 0..250 {
        let (drawn, released) = (frame(vcx), releases());
        app.update(vcx, |app, cx| app.request_animation_tick(30, cx));
        vcx.executor().advance_clock(Duration::from_millis(20));
        vcx.run_until_parked();
        if releases() > released {
            released_in.push(frame(vcx) > drawn);
        }
    }
    assert!(app.read_with(vcx, |app, _| app.frame.frame_clock_running.get()));
    assert_eq!(released_in, vec![true], "one release, in a step that drew");
    let _ = frames(vcx, 6);
    assert_eq!(releases(), before + 1, "and no other");
}

/// The image cache's idle trim frees nothing here (the demo's pictures fit
/// its idle budget): no release follows. It used to release every time.
#[gpui_kit::test]
fn an_idle_trim_that_freed_nothing_asks_for_no_release(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let before = releases();
    let cache = app
        .read_with(vcx, |app, _| app.slices.image_cache())
        .expect("the main window's image cache");
    cache.update(vcx, |cache, cx| cache.idle_trim(false, cx));
    let _ = frames(vcx, 30);
    assert_eq!(releases(), before);
}
