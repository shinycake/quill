//! The live PiP follows its feed (`calls::live_pip_follow`): a rejoin's
//! new feed swaps the player inside the same window, the gap before it
//! shows the connecting state, a stream that really ends still closes the
//! window, enlarge after a swap hands the new player back, and the live
//! story viewer and its PiP both survive a feed swap. Feeds are fed by
//! hand; the spinner's delay runs on the test executor's clock. Nothing
//! reaches Telegram; no call runs.

use super::live_pip::{
    CALL, live_pip_open, live_pip_window, open_live_story, open_stream, open_stream_pip,
    stream_player,
};
use super::pip::{new_app, settle};
use crate::ui::app::QuillApp;
use crate::ui::calls::demo_feed::DemoFeed;
use crate::ui::video_player::{VideoPlayer, shows_feed};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext, WindowId};
use quill::calls::engine::StreamState;
use quill::calls::live_video::convert::gradient_frame;
use quill::calls::live_video::{LiveEnd, LiveFrame, LiveRotation, LiveSink};
use quill::state::MAX_GROUP_CALL_REJOIN_ATTEMPTS;
use std::time::{Duration, Instant};

/// A hand-fed feed with one picture.
fn feed_with_picture() -> DemoFeed {
    let feed = DemoFeed::manual();
    let frame = LiveFrame::new(
        1,
        64,
        36,
        LiveRotation::None,
        gradient_frame(64, 36, 3),
        Instant::now(),
    );
    if let Some(frame) = frame {
        feed.source.push(frame);
    }
    feed
}

/// The PiP's own player.
fn pip_player(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<VideoPlayer> {
    app.read_with(vcx, |app, cx| {
        let view = app
            .group_call
            .live_pip
            .as_ref()?
            .window
            .as_ref()?
            .view
            .upgrade()?;
        view.read(cx).player().cloned()
    })
}

fn pip_window_id(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<WindowId> {
    app.read_with(vcx, |app, _| {
        let link = app.group_call.live_pip.as_ref()?;
        link.window.as_ref().map(|window| window.window.window_id())
    })
}

/// The rejoin's leave: the stream's feed ends as left, and no new one is
/// there yet. Returns the old feed (kept, so its drop changes nothing).
fn leave_stream_feed(app: &Entity<QuillApp>, main: &mut VisualTestContext) -> DemoFeed {
    let old = app.update_in(main, |app, _, cx| {
        cx.notify();
        app.group_call.stream.demo_feed.take()
    });
    let old = old.expect("the stream's feed");
    old.source.end(LiveEnd::Left);
    old
}

/// The rejoin's connect: a new feed with a picture. Returns its sink.
fn connect_stream_feed(app: &Entity<QuillApp>, main: &mut VisualTestContext) -> LiveSink {
    let next = feed_with_picture();
    let sink = next.sink.clone();
    app.update_in(main, |app, _, cx| {
        app.group_call.stream.demo_feed = Some(next);
        cx.notify();
    });
    sink
}

#[gpui_kit::test]
fn a_rejoin_swaps_the_pip_player_and_keeps_the_window(cx: &mut TestAppContext) {
    let (app, mut main) = new_app(cx);
    let main = &mut main;
    let mut stream = open_stream(&app, main, cx);
    let first = stream_player(&app, main).expect("the stream's player");
    let mut pip = open_stream_pip(&app, &mut stream, main, cx);
    let pip = &mut pip;
    let window = pip_window_id(&app, main).expect("the PiP's window");

    // The gap: the old feed is left and nothing new has come yet.
    let _old = leave_stream_feed(&app, main);
    settle(pip);
    settle(main);
    assert!(live_pip_open(&app, main), "a left feed does not close it");
    assert!(
        pip_player(&app, main).is_some_and(|own| own.same_as(&first)),
        "the old picture stays for the gap"
    );
    cx.executor().advance_clock(Duration::from_secs(1));
    settle(pip);
    assert!(
        pip.debug_bounds("pip-waiting").is_some(),
        "the connecting state shows"
    );
    assert!(pip.debug_bounds("live-pip-badge").is_some());

    // The new connect: the window plays the new feed.
    let sink = connect_stream_feed(&app, main);
    settle(main);
    settle(pip);
    let swapped = pip_player(&app, main).expect("the PiP plays");
    assert!(!swapped.same_as(&first), "a new player");
    assert!(shows_feed(&swapped, &sink), "on the new feed");
    assert!(
        stream_player(&app, main).is_some_and(|own| own.same_as(&swapped)),
        "kept where enlarge hands it back from"
    );
    assert_eq!(pip_window_id(&app, main), Some(window), "the same window");
    assert!(pip.debug_bounds("live-pip-badge").is_some(), "the badge");
    assert!(
        pip.debug_bounds("pip-waiting").is_none(),
        "the new picture plays"
    );
}

#[gpui_kit::test]
fn a_restart_with_no_gap_swaps_without_closing(cx: &mut TestAppContext) {
    let (app, mut main) = new_app(cx);
    let main = &mut main;
    let mut stream = open_stream(&app, main, cx);
    let first = stream_player(&app, main).expect("the stream's player");
    let mut pip = open_stream_pip(&app, &mut stream, main, cx);
    let pip = &mut pip;
    // `open_stream_feed`: the new feed is in place as the old one ends
    // as left. Whether the PiP or the app's sync sees it first, the
    // window stays and plays the new feed.
    let next = feed_with_picture();
    let sink = next.sink.clone();
    let old = app.update_in(main, |app, _, _| {
        app.group_call.stream.demo_feed.replace(next)
    });
    let old = old.expect("the stream's feed");
    old.source.end(LiveEnd::Left);
    settle(pip);
    settle(main);
    settle(pip);
    assert!(live_pip_open(&app, main), "a restart does not close it");
    let swapped = pip_player(&app, main).expect("the PiP plays");
    assert!(!swapped.same_as(&first));
    assert!(shows_feed(&swapped, &sink));
}

#[gpui_kit::test]
fn a_stream_that_really_ends_still_closes_the_pip(cx: &mut TestAppContext) {
    let (app, mut main) = new_app(cx);
    let main = &mut main;
    let mut stream = open_stream(&app, main, cx);
    let mut pip = open_stream_pip(&app, &mut stream, main, cx);
    let pip = &mut pip;
    app.update_in(main, |app, _, cx| {
        if let Some(call) = app
            .demo_session
            .as_mut()
            .and_then(|s| s.calls.active_group_call.as_mut())
        {
            call.stream = StreamState::Ended;
        }
        if let Some(feed) = app.group_call.stream.demo_feed.as_ref() {
            feed.source.end(LiveEnd::Over);
        }
        cx.notify();
    });
    // The window closes itself on the ended picture; the app forgets it.
    pip.run_until_parked();
    settle(main);
    assert!(!live_pip_open(&app, main), "the stream is over: closed");
}

#[gpui_kit::test]
fn a_left_feed_closes_the_pip_once_the_call_is_left(cx: &mut TestAppContext) {
    let (app, mut main) = new_app(cx);
    let main = &mut main;
    let mut stream = open_stream(&app, main, cx);
    let mut pip = open_stream_pip(&app, &mut stream, main, cx);
    let pip = &mut pip;
    let _old = leave_stream_feed(&app, main);
    app.update_in(main, |app, _, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.calls.left_group_call = Some(CALL);
        }
        cx.notify();
    });
    pip.run_until_parked();
    settle(main);
    assert!(!live_pip_open(&app, main), "the source is gone: closed");
}

#[gpui_kit::test]
fn enlarge_after_a_swap_hands_back_the_new_player(cx: &mut TestAppContext) {
    let (app, mut main) = new_app(cx);
    let main = &mut main;
    let mut stream = open_stream(&app, main, cx);
    let mut pip = open_stream_pip(&app, &mut stream, main, cx);
    let _old = leave_stream_feed(&app, main);
    let sink = connect_stream_feed(&app, main);
    settle(main);
    settle(&mut pip);
    let swapped = pip_player(&app, main).expect("the PiP plays");
    assert!(shows_feed(&swapped, &sink));
    pip.update(|window, cx| {
        window.hover("pip-enlarge", cx);
        window.click("pip-enlarge", cx);
    });
    pip.run_until_parked();
    settle(main);
    let window = app
        .read_with(main, |app, _| app.group_call.window)
        .expect("the stream window is back");
    let mut stream = VisualTestContext::from_window(window, cx);
    settle(&mut stream);
    settle(main);
    assert!(!live_pip_open(&app, main), "the PiP let go");
    assert!(
        stream_player(&app, main).is_some_and(|own| own.same_as(&swapped)),
        "the swapped player, nothing restarted"
    );
    assert!(stream.debug_bounds("group-call-stream-tile").is_some());
}

/// The live story's feed is swapped: the old one is left, a new one with
/// a picture takes its place. Returns the old feed and the new sink.
fn swap_story_feed(app: &Entity<QuillApp>, main: &mut VisualTestContext) -> (DemoFeed, LiveSink) {
    let next = feed_with_picture();
    let sink = next.sink.clone();
    let old = app.update_in(main, |app, _, cx| {
        cx.notify();
        app.stories.live.demo_feed.replace(next)
    });
    let old = old.expect("the story's feed");
    old.source.end(LiveEnd::Left);
    (old, sink)
}

fn story_player(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<VideoPlayer> {
    app.read_with(vcx, |app, _| app.stories.live.player.clone())
}

#[gpui_kit::test]
fn the_story_viewer_survives_a_feed_swap(cx: &mut TestAppContext) {
    let (app, mut main) = open_live_story(cx);
    let main = &mut main;
    let watching = app.read_with(main, |app, _| app.stories.live.watching);
    let first = story_player(&app, main).expect("the story plays");
    let (_old, sink) = swap_story_feed(&app, main);
    settle(main);
    app.read_with(main, |app, _| {
        assert!(app.stories.viewer.is_open(), "the viewer stays");
        assert_eq!(app.stories.live.watching, watching, "still watched");
    });
    assert!(main.debug_bounds("live-story-frame").is_some());
    let swapped = story_player(&app, main).expect("the story plays on");
    assert!(!swapped.same_as(&first), "a new player");
    assert!(shows_feed(&swapped, &sink), "on the new feed");
}

#[gpui_kit::test]
fn a_story_pip_follows_a_feed_swap_and_enlarges_with_it(cx: &mut TestAppContext) {
    let (app, mut main) = open_live_story(cx);
    let main = &mut main;
    let watching = app.read_with(main, |app, _| app.stories.live.watching);
    let first = story_player(&app, main).expect("the story plays");
    app.update_in(main, |app, window, cx| app.open_story_pip(window, cx));
    settle(main);
    let mut pip = live_pip_window(&app, main, cx);
    let window = pip_window_id(&app, main).expect("the PiP's window");
    let (_old, sink) = swap_story_feed(&app, main);
    settle(main);
    settle(&mut pip);
    let swapped = pip_player(&app, main).expect("the PiP plays");
    assert!(!swapped.same_as(&first));
    assert!(shows_feed(&swapped, &sink));
    assert_eq!(pip_window_id(&app, main), Some(window), "the same window");
    assert!(
        story_player(&app, main).is_some_and(|own| own.same_as(&swapped)),
        "kept where the viewer takes it back from"
    );
    assert_eq!(
        app.read_with(main, |app, _| app.stories.live.watching),
        watching,
        "the story stays joined"
    );
    pip.update(|window, cx| {
        window.hover("pip-enlarge", cx);
        window.click("pip-enlarge", cx);
    });
    pip.run_until_parked();
    settle(main);
    app.read_with(main, |app, _| {
        assert!(app.stories.viewer.is_open(), "the viewer is back");
        assert!(app.group_call.live_pip.is_none(), "the PiP let go");
    });
    assert!(
        story_player(&app, main).is_some_and(|own| own.same_as(&swapped)),
        "the viewer shows the swapped player"
    );
}

/// Set the tracked call's rejoin state as TDLib's update, the driver's
/// rejoin (`rejoin_group_call`) and its failure (`calls/error.rs`) do.
fn set_rejoin_state(
    app: &Entity<QuillApp>,
    main: &mut VisualTestContext,
    reconnecting: bool,
    attempts: usize,
) {
    app.update_in(main, |app, _, cx| {
        if let Some(call) = app
            .demo_session
            .as_mut()
            .and_then(|s| s.calls.active_group_call.as_mut())
        {
            call.is_joined = false;
            call.need_rejoin = true;
            call.reconnecting = reconnecting;
            call.rejoin_attempts = attempts;
        }
        cx.notify();
    });
}

#[gpui_kit::test]
fn the_pip_waits_while_rejoins_are_left_and_closes_once_they_are_spent(cx: &mut TestAppContext) {
    let (app, mut main) = new_app(cx);
    let main = &mut main;
    let mut stream = open_stream(&app, main, cx);
    let mut pip = open_stream_pip(&app, &mut stream, main, cx);
    let pip = &mut pip;
    let _old = leave_stream_feed(&app, main);
    // The network dropped the call: a rejoin is due.
    set_rejoin_state(&app, main, true, 0);
    settle(pip);
    settle(main);
    assert!(live_pip_open(&app, main), "a rejoin is due: it waits");
    // The last attempt is out and its answer pending.
    set_rejoin_state(&app, main, false, MAX_GROUP_CALL_REJOIN_ATTEMPTS);
    settle(pip);
    settle(main);
    assert!(live_pip_open(&app, main), "the last rejoin is under way");
    // It failed: no attempts are left, only the manual Rejoin.
    set_rejoin_state(&app, main, true, MAX_GROUP_CALL_REJOIN_ATTEMPTS);
    pip.run_until_parked();
    settle(main);
    assert!(!live_pip_open(&app, main), "rejoins are spent: closed");
    app.read_with(main, |app, _| {
        let call = app
            .session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            .expect("the call");
        assert_eq!(call.id, CALL, "the call stays for the manual Rejoin");
    });
}
