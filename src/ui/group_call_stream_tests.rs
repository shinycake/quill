//! UI tests for the live stream window: a channel's RTMP stream opens the
//! call window in the stream layout (title with viewers, the picture, the
//! bar), the empty-stream label sits a third of the way down, the admin
//! text shows once the server URL is known, the controls follow the mouse
//! in a normal window, and an ended stream closes the window. Nothing
//! reaches Telegram: the app has no connection and joins nothing.
use super::demo::{CALL, seed_rtmp_stream};
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
use quill::calls::engine::StreamState;
use quill::diagnostics::DiagnosticSink;
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
    assert!(label.size.width <= px(380. + 24.));
}

#[gpui_kit::test]
fn the_controls_follow_the_mouse_in_a_normal_window(cx: &mut TestAppContext) {
    let (app, _main, mut call) = open_stream(cx, StreamState::NoStream);
    let call = &mut call;
    // Out of the window: hidden.
    app.update(call, |app, _| {
        app.group_call.stream.controls = Default::default();
    });
    frames(call);
    assert!(find(call, "group-call-stream-bar").is_none());
    let viewport = find(call, "group-call-stream-viewport").expect("the picture");
    call.simulate_mouse_move(viewport.center(), None, Modifiers::default());
    frames(call);
    assert!(
        app.read_with(call, |app, _| app.group_call.stream.controls.shown()),
        "over the picture they show"
    );
    assert!(find(call, "group-call-stream-bar").is_some());
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
            let call = app
                .session()
                .and_then(|s| s.calls.active_group_call.clone())
                .expect("call");
            super::stream_label(&call, "Quill News")
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
