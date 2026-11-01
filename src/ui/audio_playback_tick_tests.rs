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

/// Tick passes over `secs` of the test clock.
fn passes_over(vcx: &mut VisualTestContext, secs: u64) -> u32 {
    let before = TICK_PASSES.with(|passes| passes.get());
    for _ in 0..secs * 10 {
        vcx.executor().advance_clock(Duration::from_millis(100));
        vcx.run_until_parked();
    }
    TICK_PASSES.with(|passes| passes.get()) - before
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
    let playing = passes_over(vcx, 2);
    assert!(
        (7..=9).contains(&playing),
        "{playing} passes in 2 s playing"
    );

    app.update(vcx, |app, _| app.pause_active_playback());
    let _ = passes_over(vcx, 1);
    assert_eq!(passes_over(vcx, 60), 0, "a paused minute runs no tick");

    // A media key or a chat-list change pokes `CHROME`: one pass each.
    quill::wake::CHROME.poke();
    assert_eq!(passes_over(vcx, 1), 1);
    quill::wake::CHROME.poke();
    assert_eq!(passes_over(vcx, 1), 1);

    // The OS thread queues Play just after a pass took the queue: it
    // plays without waiting for another wake-up.
    AFTER_DRAIN.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(|| {
            quill::media_session::push_command(quill::media_session::Command::Play);
        }));
    });
    quill::wake::CHROME.poke();
    let _ = passes_over(vcx, 1);
    assert!(
        app.read_with(vcx, |app, _| app.playback.is_playing()),
        "the Play key plays"
    );
    let resumed = passes_over(vcx, 2);
    assert!(
        (7..=10).contains(&resumed),
        "{resumed} passes in 2 s resumed"
    );
}
