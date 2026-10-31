//! Freed heap goes back to the system once Quill goes idle
//! (`docs/decisions/idle-heap-return.md`): one release after a large free,
//! once no window draws and nothing animates; none while a scroll or an
//! animation runs, and none when nothing was freed. Over the demo, on the
//! test clock; nothing reaches Telegram.

use crate::ui::app::QuillApp;
use crate::ui::frame_clock::frames_over as frames;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
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

/// A scroll draws frame after frame: five seconds of a frame every 50 ms
/// put the release off until they stop. (Scrolling the history itself
/// would also show the date pill, which fades on the wall clock.)
#[gpui_kit::test]
fn a_scroll_in_progress_puts_it_off(cx: &mut TestAppContext) {
    let (_app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let before = releases();
    free_in_a_frame(vcx);
    for _ in 0..100 {
        vcx.executor().advance_clock(Duration::from_millis(50));
        draw(vcx);
    }
    assert_eq!(releases(), before, "not while frames are drawn");
    let _ = frames(vcx, 1);
    assert_eq!(releases(), before + 1, "once they stopped");
}

/// Animated content asks the frame clock for a tick every frame, for five
/// seconds: the release waits until nothing animates.
#[gpui_kit::test]
fn an_animation_puts_it_off(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let before = releases();
    free_in_a_frame(vcx);
    for _ in 0..250 {
        app.update(vcx, |app, cx| app.request_animation_tick(30, cx));
        vcx.executor().advance_clock(Duration::from_millis(20));
        vcx.run_until_parked();
    }
    assert!(app.read_with(vcx, |app, _| app.frame.frame_clock_running.get()));
    assert_eq!(releases(), before, "not while it animates");
    let _ = frames(vcx, 6);
    assert_eq!(releases(), before + 1, "once it stopped");
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
