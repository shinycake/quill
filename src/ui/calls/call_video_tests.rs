//! UI tests for the 1:1 call window's pictures over the video call demo
//! (injected feeds, nothing reaches Telegram): both pictures draw through
//! live players, your camera mirrored, and they go with the call.
use super::call_video::CallPicture;
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::calls::live_video::LiveKey;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

/// The demo's app, its main window and the call window.
fn open_call(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-video-call")),
            )
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the app");
    let mut main = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut main);
    app.update(&mut main, |app, cx| app.sync_call_window(cx));
    frames(&mut main);
    let window = app
        .read_with(&main, |app, _| app.calls.window)
        .expect("the call window opens");
    let mut call = VisualTestContext::from_window(window, cx);
    frames(&mut call);
    (app, main, call)
}

#[gpui_kit::test]
fn your_camera_draws_through_a_live_player_mirrored(cx: &mut TestAppContext) {
    let (app, _main, mut call) = open_call(cx);
    let call = &mut call;
    assert!(
        call.debug_bounds("call-local-picture").is_some(),
        "your preview is drawn"
    );
    assert!(
        call.debug_bounds("call-remote-picture").is_some(),
        "the peer's picture is drawn"
    );
    app.read_with(call, |app, _| {
        let local = app
            .calls
            .local_player
            .clone()
            .expect("your preview's player");
        assert_eq!(
            local.live_sink().map(|sink| app
                .demo_ui
                .group_feeds
                .get(&LiveKey::Call {
                    call_id: super::call_video_demo::CALL,
                    local: true,
                    screen: false,
                })
                .is_some_and(|feed| feed.sink.same_as(&sink))),
            Some(true),
            "your preview plays your camera's feed"
        );
        assert!(local.live_mirrored(), "your preview is mirrored");
        assert!(local.live_conversions() > 0, "a frame was converted");
        let remote = app.calls.remote_player.clone().expect("the peer's player");
        assert!(!remote.live_mirrored(), "the peer is not mirrored");
        assert!(app.call_picture_drawn(CallPicture::Remote));
        assert!(app.call_picture_drawn(CallPicture::Local));
    });
}

#[gpui_kit::test]
fn turning_the_camera_off_drops_your_preview(cx: &mut TestAppContext) {
    let (app, _main, mut call) = open_call(cx);
    let call = &mut call;
    app.update(call, |app, cx| {
        if let Some(active) = app
            .demo_session
            .as_mut()
            .and_then(|s| s.calls.active_call.as_mut())
        {
            active.camera_on = false;
        }
        cx.notify();
    });
    frames(call);
    assert!(call.debug_bounds("call-local-picture").is_none());
    app.read_with(call, |app, _| {
        assert!(app.calls.local_player.is_none(), "the player went");
        assert!(app.calls.remote_player.is_some(), "the peer stays");
    });
}

#[gpui_kit::test]
fn the_players_go_with_the_call(cx: &mut TestAppContext) {
    let (app, mut main, _call) = open_call(cx);
    let main = &mut main;
    app.update(main, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.calls.active_call = None;
            session.calls.summary = None;
        }
        app.sync_call_window(cx);
    });
    frames(main);
    app.read_with(main, |app, _| {
        assert!(app.calls.window.is_none(), "the window closed");
        assert!(app.calls.local_player.is_none());
        assert!(app.calls.remote_player.is_none());
    });
}
