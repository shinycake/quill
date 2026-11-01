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

/// Tick passes over `secs` of the test clock, then the `CHROME` pokes in
/// that stretch. The signal is process-wide, so tests running beside this
/// one poke it too. A poke adds at most one pass and never removes one,
/// which keeps every bound below true under any load.
fn passes_over(vcx: &mut VisualTestContext, secs: u64) -> (u64, u64) {
    let passes = TICK_PASSES.with(|passes| passes.get());
    let pokes = quill::wake::CHROME.epoch();
    for _ in 0..secs * 10 {
        vcx.executor().advance_clock(Duration::from_millis(100));
        vcx.run_until_parked();
    }
    (
        TICK_PASSES.with(|passes| passes.get()) - passes,
        quill::wake::CHROME.epoch() - pokes,
    )
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
    let (playing, pokes) = passes_over(vcx, 2);
    assert!(
        (7..=10 + pokes).contains(&playing),
        "{playing} passes in 2 s playing, {pokes} pokes"
    );

    app.update(vcx, |app, _| app.pause_active_playback());
    let _ = passes_over(vcx, 1);
    // Each pass needs a poke of its own, or one already pending. The timer
    // this replaced ran 240 here.
    let (paused, pokes) = passes_over(vcx, 60);
    assert!(
        paused <= pokes + 1,
        "{paused} passes in a paused minute, {pokes} pokes"
    );

    // A media key or a chat-list change pokes `CHROME`: a pass each.
    for _ in 0..2 {
        quill::wake::CHROME.poke();
        let (poked, pokes) = passes_over(vcx, 1);
        assert!(
            (1..=pokes + 1).contains(&poked),
            "{poked} passes after a poke, {pokes} more pokes"
        );
    }

    // A key lands just after a pass took the queue: its poke runs another
    // pass, which takes the key, with no other wake-up.
    AFTER_DRAIN.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(|| quill::wake::CHROME.poke()));
    });
    quill::wake::CHROME.poke();
    let (drained, _) = passes_over(vcx, 1);
    assert!(drained >= 2, "{drained} passes for a key after the drain");

    app.update(vcx, |app, cx| app.toggle_active_playback(cx));
    assert!(
        app.read_with(vcx, |app, _| app.playback.is_playing()),
        "resumed"
    );
    let (resumed, pokes) = passes_over(vcx, 2);
    assert!(
        (7..=10 + pokes).contains(&resumed),
        "{resumed} passes in 2 s resumed, {pokes} pokes"
    );
}
