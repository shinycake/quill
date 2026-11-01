//! The player's progress tick over the `ready-audio` demo (a song playing
//! in chat 11): four passes a second while it plays, none while it is
//! paused until something could change what the player shows.
use super::{AFTER_DRAIN, TICK_PASSES};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use std::time::Duration;

fn open_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-audio")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the app");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    (app, vcx)
}

/// Tick passes over a stretch of the test clock, and the `CHROME` pokes in
/// it.
struct Stretch {
    passes: u64,
    pokes: u64,
}

impl Stretch {
    /// The passes are in `low..=high`, plus one for each poke: `CHROME` is
    /// process-wide, so tests running beside this one poke it too. A poke
    /// adds at most one pass and never removes one.
    #[track_caller]
    fn assert_passes(&self, low: u64, high: u64, stretch: &str) {
        let Self { passes, pokes } = *self;
        assert!(
            (low..=high + pokes).contains(&passes),
            "{passes} passes {stretch}, {pokes} pokes"
        );
    }
}

fn passes_over(vcx: &mut VisualTestContext, secs: u64) -> Stretch {
    let passes = TICK_PASSES.with(|passes| passes.get());
    let pokes = quill::wake::CHROME.epoch();
    for _ in 0..secs * 10 {
        vcx.executor().advance_clock(Duration::from_millis(100));
        vcx.run_until_parked();
    }
    Stretch {
        passes: TICK_PASSES.with(|passes| passes.get()) - passes,
        pokes: quill::wake::CHROME.epoch() - pokes,
    }
}

#[gpui_kit::test]
fn a_paused_track_ticks_only_when_something_could_change(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| app.spawn_playback_tick(cx));
    assert!(
        app.read_with(vcx, |app, _| app.playback.tick),
        "the tick runs"
    );
    passes_over(vcx, 2).assert_passes(7, 10, "in 2 s playing");

    app.update(vcx, |app, _| app.pause_active_playback());
    let _ = passes_over(vcx, 1);
    passes_over(vcx, 60).assert_passes(0, 1, "in a paused minute");

    // A media key or a chat-list change pokes `CHROME`: a pass each.
    for _ in 0..2 {
        quill::wake::CHROME.poke();
        passes_over(vcx, 1).assert_passes(1, 1, "after a poke");
    }

    app.update(vcx, |app, cx| app.toggle_active_playback(cx));
    assert!(
        app.read_with(vcx, |app, _| app.playback.is_playing()),
        "resumed"
    );
    passes_over(vcx, 2).assert_passes(7, 10, "in 2 s resumed");
}

#[gpui_kit::test]
fn a_poke_just_after_the_drain_runs_another_pass(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| app.spawn_playback_tick(cx));
    app.update(vcx, |app, _| app.pause_active_playback());
    let _ = passes_over(vcx, 1);

    AFTER_DRAIN.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(|| quill::wake::CHROME.poke()));
    });
    quill::wake::CHROME.poke();
    let drained = passes_over(vcx, 1).passes;
    assert!(drained >= 2, "{drained} passes for a poke after the drain");
}
