//! UI tests for the live stream window: a channel's RTMP stream opens the
//! call window in the stream layout (title with viewers, the picture, the
//! bar), the empty-stream label sits a third of the way down, the admin
//! text shows once the server URL is known, the controls follow the mouse
//! in a normal window, and an ended stream closes the window. Nothing
//! reaches Telegram: the app has no connection and joins nothing.
use super::StreamJoinIntent;
use super::demo::{CALL, CHANNEL, seed_rtmp_stream};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Bounds, Entity, Modifiers, Pixels, TestAppContext, VisualTestContext, point, px,
    size,
};
use quill::calls::engine::{StreamState, VideoFrame};
use quill::diagnostics::DiagnosticSink;
use quill::state::ActiveGroupCall;
use std::sync::Arc;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

/// The main window with the demo channel; the stream in `stream`. Returns
/// the app and the stream window.
fn open_stream(
    cx: &mut TestAppContext,
    stream: StreamState,
) -> (Entity<QuillApp>, VisualTestContext, VisualTestContext) {
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
                Some(ScreenshotDemo::named("ready-channels")),
            )
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
    let mut main = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut main);
    app.update_in(&mut main, |app, _, cx| {
        let sink: Arc<dyn DiagnosticSink> = app.demo_ui.sink.clone();
        let seq = &app.demo_ui.seq;
        seed_rtmp_stream(app.demo_session.as_mut().expect("demo"), stream, &sink, seq);
        cx.notify();
    });
    frames(&mut main);
    frames(&mut main);
    let window = app
        .read_with(&main, |app, _| app.group_call.window)
        .expect("the stream window opens");
    let mut call = VisualTestContext::from_window(window, cx);
    frames(&mut call);
    (app, main, call)
}

fn find(vcx: &mut VisualTestContext, selector: &'static str) -> Option<Bounds<Pixels>> {
    vcx.debug_bounds(selector)
}

#[gpui_kit::test]
fn a_live_stream_opens_the_stream_window(cx: &mut TestAppContext) {
    let (app, _main, mut call) = open_stream(cx, StreamState::NoStream);
    let call = &mut call;
    assert!(
        app.read_with(call, |app, _| app.group_call.stream.window_is_stream),
        "sized as a stream window"
    );
    let viewport = find(call, "group-call-stream-viewport").expect("the picture");
    let window = call.update(|window, _| window.viewport_size());
    // A normal window: 9 px around, 24 px for the title.
    assert_eq!(viewport.left(), px(9.));
    assert_eq!(viewport.top(), px(24.));
    assert_eq!(viewport.right(), window.width - px(9.));
    // No members list or microphone in a stream.
    assert!(find(call, "group-call-stream-title").is_some());
    // Nobody broadcasting: the label, a third of the way down.
    let label = find(call, "group-call-stream-label").expect("the empty-stream label");
    let above = f32::from(label.top());
    let below = f32::from(window.height - label.bottom());
    assert!(
        (below - 2. * above).abs() < 2.,
        "a third of the way down: {above} above, {below} below"
    );
    // `min(width - 48, 380) + 24` wide, whatever the text.
    let expected = (f32::from(window.width) - 48.).min(380.) + 24.;
    assert!((f32::from(label.size.width) - expected).abs() < 0.5);
}

#[gpui_kit::test]
fn the_controls_follow_the_mouse_in_a_normal_window(cx: &mut TestAppContext) {
    let (app, _main, mut call) = open_stream(cx, StreamState::NoStream);
    let call = &mut call;
    // Out of the window: hidden, slid below the window's edge.
    app.update(call, |app, _| {
        app.group_call.stream.controls = Default::default();
    });
    frames(call);
    let height = call.update(|window, _| window.viewport_size().height);
    let bar = find(call, "group-call-stream-bar").expect("the bar stays mounted");
    assert!(bar.top() >= height, "hidden below the window");
    let viewport = find(call, "group-call-stream-viewport").expect("the picture");
    call.simulate_mouse_move(viewport.center(), None, Modifiers::default());
    frames(call);
    assert!(
        app.read_with(call, |app, _| app.group_call.stream.controls.shown()),
        "over the picture they show"
    );
    call.simulate_mouse_move(point(px(2.), px(2.)), None, Modifiers::default());
    frames(call);
    assert!(
        !app.read_with(call, |app, _| app.group_call.stream.controls.shown()),
        "leaving the picture hides them"
    );
}

#[gpui_kit::test]
fn the_admin_sees_the_server_hint_and_an_ended_stream_closes(cx: &mut TestAppContext) {
    let (app, mut main, mut call) = open_stream(cx, StreamState::NoStream);
    let label = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.update(vcx, |app, _| {
            let facts = app.group_call_stream_facts().expect("a stream");
            super::stream_label(&facts, "Quill News")
        })
    };
    assert!(
        label(&app, &mut call).is_some_and(|text| text.contains("Quill News")),
        "a viewer reads that the channel is not broadcasting"
    );
    app.update(&mut call, |app, cx| {
        let session = app.demo_session.as_mut().expect("demo");
        if let Some(active) = session.calls.active_group_call.as_mut() {
            active.rtmp_url = Some("rtmps://dc4-1.rtmp.t.me/s/".into());
        }
        cx.notify();
    });
    assert!(
        label(&app, &mut call).is_some_and(|text| text.contains("Server URL")),
        "the admin reads the streaming-app hint"
    );
    app.update(&mut main, |app, cx| {
        let session = app.demo_session.as_mut().expect("demo");
        if let Some(active) = session.calls.active_group_call.as_mut() {
            active.stream = StreamState::Ended;
        }
        // Outside a draw: the test platform draws every dirty window in
        // one pass and cannot lose one in the middle of it.
        app.sync_group_call_window(cx);
    });
    frames(&mut main);
    assert!(
        app.read_with(&main, |app, _| app.group_call.window.is_none()),
        "the window closes with the stream"
    );
    assert_eq!(
        app.read_with(&main, |app, _| app.group_call.window_closed_by_user),
        Some(CALL)
    );
}

/// The stream's call as `updateGroupCall` tracks it before a join.
fn track_unjoined(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update(vcx, |app, cx| {
        let session = app.demo_session.as_mut().expect("demo");
        session.calls.active_group_call = Some(ActiveGroupCall {
            is_rtmp_stream: true,
            stream: StreamState::Connecting,
            ..ActiveGroupCall::fresh(CALL)
        });
        // Outside a draw, as the poll loop runs it.
        app.sync_group_call_window(cx);
    });
    frames(vcx);
}

fn joined(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            .is_some_and(|call| call.is_joined)
    })
}

#[gpui_kit::test]
fn leaving_then_reopening_the_stream_joins_it_again(cx: &mut TestAppContext) {
    let (app, mut main, _call) = open_stream(cx, StreamState::Live);
    let main = &mut main;
    // Leave: the call is no longer tracked and the window closes.
    app.update(main, |app, cx| {
        app.leave_active_group_call(cx);
        app.sync_group_call_window(cx);
    });
    frames(main);
    assert!(app.read_with(main, |app, _| app.group_call.window.is_none()));
    // Right after Leave the call comes back unjoined (updateGroupCall
    // with is_joined false): nothing joins it by itself.
    track_unjoined(&app, main);
    assert!(!joined(&app, main), "no join you did not ask for");
    // Reopen it from the channel: the window joins once it is tracked.
    app.update_in(main, |app, window, cx| {
        app.start_or_join_video_chat(quill::ids::ChatId(CHANNEL), window, cx);
    });
    app.update(main, |app, cx| app.sync_group_call_window(cx));
    frames(main);
    assert!(joined(&app, main), "reopening joins again");
    assert!(
        app.read_with(main, |app, _| app.group_call.stream.pending_join.is_none()),
        "the wish is used once"
    );
    // Leave and reopen once more: it still joins.
    app.update(main, |app, cx| {
        app.leave_active_group_call(cx);
        app.sync_group_call_window(cx);
    });
    app.update(main, |app, _| {
        app.note_stream_join_intent(StreamJoinIntent::Call(CALL))
    });
    track_unjoined(&app, main);
    assert!(joined(&app, main), "every reopen joins");
}

#[gpui_kit::test]
fn an_unjoined_stream_offers_join(cx: &mut TestAppContext) {
    let (app, mut main, _call) = open_stream(cx, StreamState::Live);
    app.update(&mut main, |app, cx| {
        app.leave_active_group_call(cx);
        app.sync_group_call_window(cx);
    });
    frames(&mut main);
    track_unjoined(&app, &mut main);
    let window = app
        .read_with(&main, |app, _| app.group_call.window)
        .expect("the unjoined stream's window");
    let mut call = VisualTestContext::from_window(window, cx);
    frames(&mut call);
    let button = find(&mut call, "group-call-stream-label").expect("the Join label");
    assert!(!joined(&app, &mut call));
    // A failed join shows why, and the button tries again.
    app.update(&mut call, |app, cx| {
        let session = app.demo_session.as_mut().expect("demo");
        session.calls.group_call_error = Some("GROUPCALL_JOIN_MISSING".into());
        cx.notify();
    });
    frames(&mut call);
    app.update(&mut call, |app, cx| app.join_stream(CALL, cx));
    frames(&mut call);
    assert!(joined(&app, &mut call), "Join joins");
    assert!(
        app.read_with(&call, |app, _| app
            .session()
            .is_some_and(|s| s.calls.group_call_error.is_none())),
        "a new try clears the old reason"
    );
    let _ = button;
}

#[gpui_kit::test]
fn the_settings_box_opens_and_esc_closes_it(cx: &mut TestAppContext) {
    let (app, _main, mut call) = open_stream(cx, StreamState::Live);
    let call = &mut call;
    app.update(call, |app, cx| {
        app.open_stream_overlay(super::StreamOverlay::Settings, cx)
    });
    frames(call);
    assert!(
        find(call, "group-call-stream-box").is_some(),
        "Settings opens"
    );
    call.simulate_keystrokes("escape");
    frames(call);
    assert!(
        app.read_with(call, |app, _| app.group_call.stream.overlay.is_none()),
        "Esc closes the box first"
    );
}

/// The stream picture with the main window in the background: only the
/// stream window draws, so nothing waits for the main window's sweep.
/// Before this, 200 frames at 1280 × 720 kept 199 images (699 MiB).
#[gpui_kit::test]
fn stream_pictures_stay_bounded_with_the_main_window_idle(cx: &mut TestAppContext) {
    let (app, _main, mut call) = open_stream(cx, StreamState::Live);
    let call = &mut call;
    for seq in 0..200u64 {
        app.update(call, |app, _| {
            app.group_call.stream.demo_frame = Some(Arc::new(VideoFrame {
                seq: seq + 10,
                width: 1280,
                height: 720,
                rgba: vec![0; 1280 * 720 * 4],
                is_local: false,
                participant_user_id: None,
                is_screen: false,
            }));
        });
        call.update(|window, cx| window.render_frame(cx));
        call.run_until_parked();
    }
    call.update(|window, cx| window.render_frame(cx));
    let (made, alive) = app.read_with(call, |app, _| {
        let made = &app.group_call.stream.picture.made;
        (
            made.len(),
            made.iter().filter(|image| image.strong_count() > 0).count(),
        )
    });
    eprintln!("MEASURE made {made} images, {alive} alive");
    assert!(made >= 100, "the frames were converted: {made}");
    assert!(
        alive <= 2,
        "only the shown image and the one before: {alive}"
    );
}

#[gpui_kit::test]
fn a_wish_for_a_call_that_another_call_replaced_is_dropped(cx: &mut TestAppContext) {
    let (app, mut main, _call) = open_stream(cx, StreamState::Live);
    let main = &mut main;
    // Asked for call 999 while the stream is tracked: kept while the
    // same call is tracked.
    app.update(main, |app, cx| {
        app.note_stream_join_intent(StreamJoinIntent::Call(999));
        app.sync_group_call_window(cx);
    });
    assert!(app.read_with(main, |app, _| app.group_call.stream.pending_join.is_some()));
    // Another call takes its place: the wish goes.
    app.update(main, |app, cx| {
        let session = app.demo_session.as_mut().expect("demo");
        session.calls.active_group_call = Some(ActiveGroupCall::fresh(777));
        app.sync_group_call_window(cx);
    });
    assert!(app.read_with(main, |app, _| app.group_call.stream.pending_join.is_none()));
}

#[gpui_kit::test]
fn a_join_that_never_lands_stops_waiting(cx: &mut TestAppContext) {
    let (app, mut main, _call) = open_stream(cx, StreamState::Live);
    app.update(&mut main, |app, cx| {
        app.leave_active_group_call(cx);
        app.sync_group_call_window(cx);
    });
    track_unjoined(&app, &mut main);
    let window = app
        .read_with(&main, |app, _| app.group_call.window)
        .expect("the unjoined stream's window");
    let mut call = VisualTestContext::from_window(window, cx);
    let error = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| {
            app.session().and_then(|s| s.calls.group_call_error.clone())
        })
    };
    // The timer runs out with the call still unjoined.
    app.update(&mut call, |app, cx| {
        app.group_call.stream.joining = Some(CALL);
        app.stream_join_ended(CALL, cx);
    });
    assert!(app.read_with(&call, |app, _| app.group_call.stream.joining.is_none()));
    assert!(error(&app, &mut call).is_some(), "the failure shows");
    // The native connect fails without a join error: the next draw stops
    // the spinner and says so.
    app.update(&mut call, |app, cx| {
        let session = app.demo_session.as_mut().expect("demo");
        session.calls.group_call_error = None;
        if let Some(active) = session.calls.active_group_call.as_mut() {
            active.transport_error = Some("connect failed".into());
        }
        app.group_call.stream.joining = Some(CALL);
        cx.notify();
    });
    frames(&mut call);
    assert!(app.read_with(&call, |app, _| app.group_call.stream.joining.is_none()));
    assert!(error(&app, &mut call).is_some());
}
