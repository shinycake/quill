//! An idle window draws nothing. With the usual surfaces up and nothing
//! changing on screen, a simulated minute passes without a frame in an
//! inactive window, and in an active one without a frame other than the
//! caret's. Each frame costs CPU and keeps the display link from idling,
//! which holds a spare window-sized drawable on macOS. Over the demo;
//! nothing reaches Telegram. The fixtures keep their scroll offset once
//! open: the kit scrollbar times its fade by the wall clock, which the
//! test clock outruns (`docs/decisions/idle-inactive-redraw.md`).

use crate::ui::app::QuillApp;
use crate::ui::frame_clock::frames_over as frames;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};

fn open(cx: &mut TestAppContext, kind: &'static str) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named(kind);
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    vcx.update(|window, cx| window.render_frame(cx));
    (app, vcx)
}

fn activate(vcx: &mut VisualTestContext) {
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
}

fn focus_composer(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, window, cx| {
        app.composer
            .update(cx, |composer, cx| composer.focus(window, cx));
    });
    vcx.run_until_parked();
}

/// Behind another app, an open chat draws nothing over a minute: the chat
/// list, the history and the focused composer, whose caret does not blink
/// in an inactive window.
#[gpui_kit::test]
fn an_open_chat_draws_nothing_behind_another_app(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-chats");
    let vcx = &mut vcx;
    activate(vcx);
    focus_composer(&app, vcx);
    let _ = frames(vcx, 5);
    vcx.deactivate_window();
    let _ = frames(vcx, 5);
    let drawn = frames(vcx, 60);
    assert!(drawn.is_empty(), "drawn behind another app: {drawn:?}");
}

/// In front, the focused composer's caret is all that draws: the composer
/// alone, twice a second.
#[gpui_kit::test]
fn in_front_only_the_caret_draws(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-chats");
    let vcx = &mut vcx;
    activate(vcx);
    focus_composer(&app, vcx);
    let _ = frames(vcx, 5);
    let drawn = frames(vcx, 60);
    let others: Vec<_> = drawn
        .keys()
        .filter(|kind| !["(window frame)", "composer", "animation layer"].contains(kind))
        .collect();
    assert!(others.is_empty(), "drawn besides the caret: {drawn:?}");
    let frames = drawn.get("(window frame)").copied().unwrap_or(0);
    assert!((100..=121).contains(&frames), "the caret blinks: {drawn:?}");
}

/// A dialog over the chat (Settings) draws nothing behind another app.
#[gpui_kit::test]
fn a_dialog_draws_nothing_behind_another_app(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-chats");
    let vcx = &mut vcx;
    activate(vcx);
    app.update(vcx, |app, cx| {
        app.settings.open = true;
        cx.notify();
    });
    let _ = frames(vcx, 5);
    vcx.deactivate_window();
    let _ = frames(vcx, 5);
    let drawn = frames(vcx, 60);
    assert!(drawn.is_empty(), "drawn behind another app: {drawn:?}");
}

/// A paused track's seek bars and player bar stay as they are, so nothing
/// draws, in front or behind another app; the progress tick redrew them
/// four times a second. Playing again moves them again.
#[gpui_kit::test]
fn a_paused_track_draws_nothing(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx, "ready-voice-playing");
    let vcx = &mut vcx;
    activate(vcx);
    app.update(vcx, |app, cx| app.toggle_active_playback(cx));
    let paused = app.read_with(vcx, |app, _| {
        app.active_playback_id().is_some() && !app.playback.is_playing()
    });
    assert!(paused, "the voice note is paused, its bar still up");
    let _ = frames(vcx, 1);
    let drawn = frames(vcx, 60);
    assert!(drawn.is_empty(), "drawn in front: {drawn:?}");
    vcx.deactivate_window();
    let _ = frames(vcx, 1);
    let drawn = frames(vcx, 60);
    assert!(drawn.is_empty(), "drawn behind another app: {drawn:?}");

    activate(vcx);
    app.update(vcx, |app, cx| app.toggle_active_playback(cx));
    assert!(app.read_with(vcx, |app, _| app.playback.is_playing()));
    let drawn = frames(vcx, 1);
    assert!(
        drawn.contains_key("conversation"),
        "the seek bar moves: {drawn:?}"
    );
}
