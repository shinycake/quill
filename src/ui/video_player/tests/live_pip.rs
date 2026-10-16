//! Live Picture-in-Picture (`calls::live_pip`) on a stream fed by hand and
//! on the live story demo: it opens from the stream window with the LIVE
//! badge and no scrubber, enlarge goes back to the stream window with the
//! same player, one sound plays at a time with a file PiP, and it closes
//! on the call's end and on lock while closing it keeps the call. A live
//! story's PiP keeps the story joined until it closes. Nothing reaches
//! Telegram; no call runs.

use super::pip::{new_app, open_pip_on, settle};
use super::{Script, queue_backend};
use crate::ui::app::QuillApp;
use crate::ui::calls::demo_feed::DemoFeed;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use crate::ui::video_player::VideoPlayer;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::calls::engine::{GroupCallMode, StreamState};
use quill::calls::live_video::convert::gradient_frame;
use quill::calls::live_video::{LiveFrame, LiveRotation};
use quill::ids::{ChatId, MessageId};
use quill::state::{ActiveGroupCall, VideoChatInfo};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

const CALL: i32 = 4601;

/// A joined RTMP stream on chat 11 whose feed has one picture.
fn seed_stream(app: &mut QuillApp) {
    if let Some(session) = app.demo_session.as_mut() {
        if let Some(chat) = session.chats.get_mut(&11) {
            chat.video_chat = Some(VideoChatInfo {
                group_call_id: CALL,
                has_participants: true,
                default_participant_id: None,
            });
        }
        session.calls.active_group_call = Some(ActiveGroupCall {
            is_rtmp_stream: true,
            is_joined: true,
            stream_mode: GroupCallMode::Rtmp,
            stream: StreamState::Live,
            viewer_count: 12_480,
            ..ActiveGroupCall::fresh(CALL)
        });
    }
    let feed = DemoFeed::manual();
    let frame = LiveFrame::new(
        1,
        64,
        36,
        LiveRotation::None,
        gradient_frame(64, 36, 0),
        Instant::now(),
    );
    if let Some(frame) = frame {
        feed.source.push(frame);
    }
    app.group_call.stream.demo_feed = Some(feed);
}

/// The stream window over the seeded stream, drawn (its player made).
fn open_stream(
    app: &Entity<QuillApp>,
    main: &mut VisualTestContext,
    cx: &mut TestAppContext,
) -> VisualTestContext {
    app.update_in(main, |app, _, cx| {
        seed_stream(app);
        cx.notify();
    });
    settle(main);
    let window = app
        .read_with(main, |app, _| app.group_call.window)
        .expect("the stream window opens");
    let mut stream = VisualTestContext::from_window(window, cx);
    settle(&mut stream);
    assert!(stream_player(app, main).is_some(), "the stream plays");
    stream
}

fn stream_player(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<VideoPlayer> {
    app.read_with(vcx, |app, _| app.group_call.stream.player.clone())
}

/// The live PiP's window, drawn.
fn live_pip_window(
    app: &Entity<QuillApp>,
    main: &mut VisualTestContext,
    cx: &mut TestAppContext,
) -> VisualTestContext {
    let handle = app
        .read_with(main, |app, _| {
            app.group_call
                .live_pip
                .as_ref()
                .and_then(|link| link.window.as_ref())
                .map(|window| window.window)
        })
        .expect("the live PiP opened");
    let mut pip = VisualTestContext::from_window(handle.into(), cx);
    settle(&mut pip);
    pip
}

/// The stream window's PiP button.
fn open_stream_pip(
    app: &Entity<QuillApp>,
    stream: &mut VisualTestContext,
    main: &mut VisualTestContext,
    cx: &mut TestAppContext,
) -> VisualTestContext {
    app.update_in(stream, |app, window, cx| app.open_stream_pip(window, cx));
    stream.run_until_parked();
    settle(main);
    live_pip_window(app, main, cx)
}

fn live_pip_open(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| app.group_call.live_pip.is_some())
}

#[gpui_kit::test]
fn a_stream_pip_shows_the_live_badge_and_no_scrubber(cx: &mut TestAppContext) {
    let (app, mut main) = new_app(cx);
    let main = &mut main;
    let mut stream = open_stream(&app, main, cx);
    let player = stream_player(&app, main).expect("the stream's player");
    let mut pip = open_stream_pip(&app, &mut stream, main, cx);
    let pip = &mut pip;
    pip.update(|window, cx| window.hover("pip-volume", cx));
    settle(pip);
    assert!(
        pip.debug_bounds("live-pip-badge").is_some(),
        "LIVE and viewers"
    );
    assert!(pip.debug_bounds("live-pill").is_some());
    assert!(pip.debug_bounds("live-pip-mute").is_some(), "mute");
    assert!(pip.debug_bounds("pip-playback").is_none(), "no scrubber");
    assert!(pip.debug_bounds("pip-times").is_none(), "no times");
    app.read_with(main, |app, cx| {
        let link = app.group_call.live_pip.as_ref().expect("the link");
        let view = link
            .window
            .as_ref()
            .and_then(|window| window.view.upgrade())
            .expect("its view");
        let pip = view.read(cx);
        assert!(pip.is_live());
        assert!(pip.player().is_some_and(|own| own.same_as(&player)));
        assert!(app.group_call.window.is_none(), "the stream window closed");
        let call = app
            .session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            .expect("the call");
        assert!(call.is_joined, "the call stays");
    });
    // The stream window being closed does not drop the player PiP plays.
    assert!(
        stream_player(&app, main).is_some_and(|own| own.same_as(&player)),
        "the stream's player lives on"
    );
}

#[gpui_kit::test]
fn mute_goes_through_the_call_and_the_button_follows(cx: &mut TestAppContext) {
    let (app, mut main) = new_app(cx);
    let main = &mut main;
    let mut stream = open_stream(&app, main, cx);
    let mut pip = open_stream_pip(&app, &mut stream, main, cx);
    let pip = &mut pip;
    assert!(!app.read_with(main, |app, _| app.stream_muted()));
    pip.update(|window, cx| {
        window.hover("pip-volume", cx);
        window.click("pip-volume", cx);
    });
    settle(pip);
    settle(main);
    assert!(
        app.read_with(main, |app, _| app.stream_muted()),
        "the stream's sound is muted for you"
    );
    assert!(live_pip_open(&app, main), "the window stays");
}

#[gpui_kit::test]
fn enlarge_goes_back_to_the_stream_window_with_the_same_player(cx: &mut TestAppContext) {
    let (app, mut main) = new_app(cx);
    let main = &mut main;
    let mut stream = open_stream(&app, main, cx);
    let player = stream_player(&app, main).expect("the stream's player");
    let mut pip = open_stream_pip(&app, &mut stream, main, cx);
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
        stream_player(&app, main).is_some_and(|own| own.same_as(&player)),
        "the same player, nothing restarted"
    );
    assert!(stream.debug_bounds("group-call-stream-tile").is_some());
}

#[gpui_kit::test]
fn closing_keeps_the_call_and_lets_the_stream_go(cx: &mut TestAppContext) {
    let (app, mut main) = new_app(cx);
    let main = &mut main;
    let mut stream = open_stream(&app, main, cx);
    let mut pip = open_stream_pip(&app, &mut stream, main, cx);
    pip.update(|window, cx| {
        window.hover("pip-close", cx);
        window.click("pip-close", cx);
    });
    pip.run_until_parked();
    settle(main);
    assert!(!live_pip_open(&app, main));
    app.read_with(main, |app, _| {
        let call = app
            .session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            .expect("the call");
        assert!(call.is_joined, "closing PiP does not leave the call");
        assert!(app.group_call.window.is_none(), "the window stays closed");
        assert!(app.group_call.stream.player.is_none(), "its player went");
    });
}

#[gpui_kit::test]
fn the_live_pip_closes_when_the_call_ends(cx: &mut TestAppContext) {
    let (app, mut main) = new_app(cx);
    let main = &mut main;
    let mut stream = open_stream(&app, main, cx);
    let _pip = open_stream_pip(&app, &mut stream, main, cx);
    app.update_in(main, |app, _, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.calls.active_group_call = None;
        }
        cx.notify();
    });
    settle(main);
    assert!(!live_pip_open(&app, main));
    assert!(stream_player(&app, main).is_none(), "the player went");
}

#[gpui_kit::test]
fn locking_quill_closes_the_live_pip(cx: &mut TestAppContext) {
    let (app, mut main) = new_app(cx);
    let main = &mut main;
    let mut stream = open_stream(&app, main, cx);
    let _pip = open_stream_pip(&app, &mut stream, main, cx);
    app.update_in(main, |app, _, cx| {
        app.account.passcode.locked = true;
        app.check_video_pip(cx);
    });
    settle(main);
    assert!(!live_pip_open(&app, main));
}

#[gpui_kit::test]
fn one_sound_at_a_time_with_a_file_pip(cx: &mut TestAppContext) {
    let script = Rc::new(RefCell::new(Script {
        ready: true,
        picture: true,
        ..Script::default()
    }));
    let (app, mut main, viewer_player) = open_pip_on(cx, &script, |_| {}, MessageId(204), 0);
    // Only the PiP holds the file's player.
    drop(viewer_player);
    let main = &mut main;
    assert!(app.read_with(main, |app, _| app.viewer.pip.is_some()));
    // A live PiP closes the file's.
    let mut stream = open_stream(&app, main, cx);
    let _pip = open_stream_pip(&app, &mut stream, main, cx);
    settle(main);
    assert!(live_pip_open(&app, main));
    assert!(
        app.read_with(main, |app, _| app.viewer.pip.is_none()),
        "the file PiP closed"
    );
    assert!(script.borrow().dropped, "its sound stopped");
    // A file PiP closes the live one.
    let script = Rc::new(RefCell::new(Script {
        ready: true,
        picture: true,
        ..Script::default()
    }));
    queue_backend(&script);
    app.update_in(main, |app, _, cx| {
        app.open_media_viewer_at(ChatId(11), MessageId(204), 0, cx);
    });
    settle(main);
    app.update_in(main, |app, _, cx| app.open_video_pip(cx));
    settle(main);
    assert!(app.read_with(main, |app, _| app.viewer.pip.is_some()));
    assert!(!live_pip_open(&app, main), "the live PiP closed");
    // Another sound (a story, a voice message) closes a live PiP too.
    let mut stream_window = app
        .read_with(main, |app, _| app.group_call.window)
        .map(|window| VisualTestContext::from_window(window, cx));
    if let Some(stream) = stream_window.as_mut() {
        settle(stream);
        let _pip = open_stream_pip(&app, stream, main, cx);
        app.update_in(main, |app, _, cx| app.sound_starts(cx));
        settle(main);
        assert!(!live_pip_open(&app, main), "another sound closed it");
    }
}

/// The live story demo, its viewer open on the story.
fn open_live_story(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = Rc::new(RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-live-story")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    settle(&mut vcx);
    (app, vcx)
}

#[gpui_kit::test]
fn a_live_story_pip_keeps_the_story_until_it_closes(cx: &mut TestAppContext) {
    let (app, mut main) = open_live_story(cx);
    let main = &mut main;
    let watching = app.read_with(main, |app, _| app.stories.live.watching);
    assert!(watching.is_some(), "the demo watches its live story");
    assert!(main.debug_bounds("live-story-frame").is_some());
    app.update_in(main, |app, window, cx| app.open_story_pip(window, cx));
    settle(main);
    let mut pip = live_pip_window(&app, main, cx);
    app.read_with(main, |app, _| {
        assert!(!app.stories.viewer.is_open(), "the viewer closed");
        assert_eq!(
            app.stories.live.watching, watching,
            "the story stays joined"
        );
    });
    assert!(pip.debug_bounds("live-pip-badge").is_some());
    // Enlarge: the viewer is back on the story, which takes the player.
    pip.update(|window, cx| {
        window.hover("pip-enlarge", cx);
        window.click("pip-enlarge", cx);
    });
    pip.run_until_parked();
    settle(main);
    app.read_with(main, |app, _| {
        assert!(app.stories.viewer.is_open(), "the viewer is back");
        assert_eq!(app.stories.live.watching, watching);
        assert!(app.group_call.live_pip.is_none(), "the PiP let go");
    });
    assert!(main.debug_bounds("live-story-frame").is_some());
    // Again, then close: the story is left.
    app.update_in(main, |app, window, cx| app.open_story_pip(window, cx));
    settle(main);
    let mut pip = live_pip_window(&app, main, cx);
    pip.update(|window, cx| {
        window.hover("pip-close", cx);
        window.click("pip-close", cx);
    });
    pip.run_until_parked();
    settle(main);
    app.read_with(main, |app, _| {
        assert!(app.group_call.live_pip.is_none());
        assert_eq!(app.stories.live.watching, None, "closing left the story");
    });
}
