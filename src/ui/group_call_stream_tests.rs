//! UI tests for the live stream window: a channel's RTMP stream opens the
//! call window in the stream layout (title with viewers, the picture, the
//! bar), the empty-stream label sits a third of the way down, the admin
//! text shows once the server URL is known, the controls follow the mouse
//! in a normal window, and an ended stream closes the window; the live
//! controller (no scrubber, the LIVE pill, the keys), the stall, paused
//! and no-stream states, and full screen hiding the controls and the
//! cursor. Nothing reaches Telegram: the app has no connection and joins
//! nothing.
use super::StreamJoinIntent;
use super::demo::{CALL, CHANNEL, seed_rtmp_stream};
use crate::ui::app::QuillApp;
use crate::ui::calls::demo_feed::DemoFeed;
use crate::ui::frame_clock::take_slice_renders;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use crate::ui::video_player::controls::cursor_hides;
use crate::ui::video_player::{PlayState, STALL_AFTER, bgra_only, take_made_images};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Bounds, Entity, Modifiers, Pixels, TestAppContext, VisualTestContext, point, px,
    size,
};
use quill::calls::engine::StreamState;
use quill::calls::live_video::convert::i420_size;
use quill::calls::live_video::{LiveFrame, LiveRotation, LiveSource, LiveState};
use quill::diagnostics::DiagnosticSink;
use quill::state::ActiveGroupCall;
use std::sync::Arc;
use std::time::{Duration, Instant};

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

/// The stream's feed, made a manual one; returns its source.
fn manual_feed(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> LiveSource {
    let feed = DemoFeed::manual();
    let source = feed.source.clone();
    app.update(vcx, |app, cx| {
        app.group_call.stream.demo_feed = Some(feed);
        cx.notify();
    });
    frames(vcx);
    source
}

fn black_frame(seq: u64, width: u16, height: u16) -> LiveFrame {
    let mut planes = vec![16u8; i420_size(width, height)];
    let luma = usize::from(width) * usize::from(height);
    for byte in planes.iter_mut().skip(luma) {
        *byte = 128;
    }
    LiveFrame::new(
        seq,
        width,
        height,
        LiveRotation::None,
        planes,
        Instant::now(),
    )
    .expect("frame")
}

/// The stream picture with the main window in the background: only the
/// stream window draws, so nothing waits for the main window's sweep.
/// Before the live player, 200 frames at 1280 × 720 kept 199 images
/// (699 MiB). Now a frame is a pooled NV12 surface on macOS (no image
/// at all) and elsewhere an image the waker drops two pictures later.
#[gpui_kit::test]
fn stream_pictures_stay_bounded_with_the_main_window_idle(cx: &mut TestAppContext) {
    let (app, _main, mut call) = open_stream(cx, StreamState::Live);
    let call = &mut call;
    let source = manual_feed(&app, call);
    let _ = take_made_images();
    for seq in 0..200u64 {
        source.push(black_frame(seq + 10, 1280, 720));
        call.run_until_parked();
        call.update(|window, cx| window.render_frame(cx));
    }
    call.run_until_parked();
    call.update(|window, cx| window.render_frame(cx));
    let player = app
        .read_with(call, |app, _| app.group_call.stream.player.clone())
        .expect("the stream has a player");
    let made = take_made_images();
    let alive = made.iter().filter(|image| image.strong_count() > 0).count();
    eprintln!(
        "MEASURE made {} images, {alive} alive, {} redraws",
        made.len(),
        player.live_asks()
    );
    assert!(player.live_asks() >= 190, "the frames were drawn");
    if !bgra_only() {
        assert!(made.is_empty(), "surfaces, not images: {}", made.len());
    } else {
        assert!(
            made.len() >= 190,
            "the frames were converted: {}",
            made.len()
        );
    }
    // The shown image, the one before, and the one the last draw retired
    // (dropped at the next wake, or when the player is released).
    assert!(alive <= 3, "only the last pictures: {alive}");
    call.update(|_, cx| player.release_live(cx));
    let alive = made.iter().filter(|image| image.strong_count() > 0).count();
    // The player's own picture (what it draws next) is all that is left.
    assert!(alive <= 1, "released with the player: {alive}");
}

/// At 30 fps the stream window redraws its picture, not the panel above
/// it (title, controls, labels replay their cached frame).
#[gpui_kit::test]
fn a_stream_frame_redraws_only_the_picture(cx: &mut TestAppContext) {
    let (app, _main, mut call) = open_stream(cx, StreamState::Live);
    let call = &mut call;
    let source = manual_feed(&app, call);
    source.push(black_frame(1, 640, 360));
    frames(call);
    let _ = take_slice_renders();
    for seq in 2..32u64 {
        source.push(black_frame(seq, 640, 360));
        call.run_until_parked();
        // A display frame: dirty views only (`render_frame` would
        // refresh the whole window, past every cache).
        call.update(|window, cx| window.draw(cx).clear(cx));
    }
    let renders = take_slice_renders();
    eprintln!("MEASURE slice renders for 30 frames: {renders:?}");
    let picture = renders.get("stream-picture").copied().unwrap_or(0);
    let panel = renders.get("group-call-panel").copied().unwrap_or(0);
    assert!(picture >= 30, "the picture redrew per frame: {picture}");
    assert_eq!(panel, 0, "the panel replayed");
}

/// Before the first frame the picture is the chat's photo blurred; the
/// first frame replaces it.
#[gpui_kit::test]
fn the_first_frame_replaces_the_blurred_photo(cx: &mut TestAppContext) {
    let (app, _main, mut call) = open_stream(cx, StreamState::Live);
    let call = &mut call;
    let source = manual_feed(&app, call);
    let player = app
        .read_with(call, |app, _| app.group_call.stream.player.clone())
        .expect("the stream has a player");
    assert!(!player.has_live_frame());
    assert_eq!(player.state(), crate::ui::video_player::PlayState::Opening);
    source.push(black_frame(1, 640, 360));
    frames(call);
    assert!(player.has_live_frame());
    assert_eq!(player.state(), crate::ui::video_player::PlayState::Playing);
    assert_eq!(player.picture_size(), Some((640, 360)));
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

/// The row id the stream window's live controller and overlays use.
fn row() -> u64 {
    u64::from(CALL.unsigned_abs())
}

fn present(vcx: &mut VisualTestContext, id: (&'static str, u64)) -> bool {
    vcx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some()
    })
}

/// The live controller: the LIVE pill by the viewers in the title, mute
/// and full screen on the bar, and none of the file controller's
/// scrubber, play button or time.
#[gpui_kit::test]
fn the_live_controller_has_no_scrubber_and_shows_the_badge(cx: &mut TestAppContext) {
    let (app, _main, mut call) = open_stream(cx, StreamState::Live);
    let call = &mut call;
    app.update(call, |app, cx| {
        app.group_call.stream.controls.show_now();
        cx.notify();
    });
    frames(call);
    let title = find(call, "group-call-stream-title").expect("the title");
    let pill = find(call, "live-pill").expect("the LIVE pill");
    assert!(
        pill.left() > title.left() && pill.right() < title.right(),
        "the pill sits in the title, before the count"
    );
    assert_eq!(pill.size.height, px(11.), "groupCallMessageBadge height");
    assert!(find(call, "group-call-stream-bar").is_some(), "the bar");
    assert!(present(call, ("live-mute", row())), "mute");
    assert!(
        find(call, "group-call-stream-fullscreen").is_some(),
        "full screen"
    );
    assert!(present(call, ("live-pip", row())), "the live PiP");
    for id in ["viewer-scrubber", "media-viewer-play", "media-viewer-speed"] {
        assert!(!present(call, (id, row())), "{id} is not on a live picture");
    }
}

/// Space does nothing, M mutes, F toggles full screen and Esc leaves it.
#[gpui_kit::test]
fn the_live_keys_mute_and_toggle_full_screen(cx: &mut TestAppContext) {
    let (app, _main, mut call) = open_stream(cx, StreamState::Live);
    let call = &mut call;
    let muted = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| app.stream_muted())
    };
    let full = |vcx: &mut VisualTestContext| vcx.update(|window, _| window.is_fullscreen());
    assert!(!muted(&app, call));
    call.simulate_keystrokes("space");
    frames(call);
    assert!(!muted(&app, call) && !full(call), "Space does nothing");
    call.simulate_keystrokes("m");
    frames(call);
    assert!(muted(&app, call), "M mutes");
    call.simulate_keystrokes("m");
    frames(call);
    assert!(!muted(&app, call), "M again unmutes");
    call.simulate_keystrokes("f");
    frames(call);
    assert!(full(call), "F goes full screen");
    call.simulate_keystrokes("escape");
    frames(call);
    assert!(!full(call), "Esc leaves full screen");
    assert!(
        app.read_with(call, |app, _| app.group_call.window.is_some()),
        "and does not close the window"
    );
}

/// A feed that stops for the stall time keeps its last picture under the
/// spinner; "Connecting…" follows 5 s after the last frame.
#[gpui_kit::test]
fn a_stalled_stream_keeps_its_picture_under_the_spinner(cx: &mut TestAppContext) {
    let (app, _main, mut call) = open_stream(cx, StreamState::Live);
    let call = &mut call;
    let source = manual_feed(&app, call);
    let player = app
        .read_with(call, |app, _| app.group_call.stream.player.clone())
        .expect("the stream has a player");
    let t0 = Instant::now();
    player.set_live_clock(t0);
    source.push(black_frame(1, 640, 360));
    frames(call);
    assert_eq!(player.state(), PlayState::Playing);
    assert!(find(call, "live-stalled").is_none());
    let size = player.picture_size();
    // The stall time passes without a frame.
    player.set_live_clock(t0 + STALL_AFTER + Duration::from_millis(200));
    app.update(call, |_, cx| cx.notify());
    frames(call);
    assert_eq!(player.state(), PlayState::Buffering);
    assert!(find(call, "live-stalled").is_some(), "the stall overlay");
    assert!(find(call, "live-spinner").is_some(), "with the spinner");
    assert!(
        find(call, "live-connecting").is_none(),
        "not yet Connecting"
    );
    assert!(player.has_live_frame(), "the picture stays");
    assert_eq!(player.picture_size(), size);
    // 5 s after the last frame: "Connecting…".
    cx.executor().advance_clock(Duration::from_millis(3600));
    frames(call);
    assert!(find(call, "live-connecting").is_some(), "Connecting… shows");
}

/// The broadcaster paused: the blurred picture under the pause icon and
/// "Video is paused".
#[gpui_kit::test]
fn a_paused_stream_shows_the_paused_overlay(cx: &mut TestAppContext) {
    let (app, _main, mut call) = open_stream(cx, StreamState::Live);
    let call = &mut call;
    let source = manual_feed(&app, call);
    let player = app
        .read_with(call, |app, _| app.group_call.stream.player.clone())
        .expect("the stream has a player");
    source.push(black_frame(1, 640, 360));
    frames(call);
    let sink = player.live_sink().expect("a live player");
    sink.set_state(LiveState::Paused);
    app.update(call, |_, cx| cx.notify());
    frames(call);
    assert_eq!(player.state(), PlayState::Paused);
    assert!(find(call, "live-paused").is_some(), "the paused overlay");
    sink.set_state(LiveState::Active);
    app.update(call, |_, cx| cx.notify());
    frames(call);
    assert!(find(call, "live-paused").is_none());
}

fn controls_shown(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| app.group_call.stream.controls.shown())
}

fn hide_pending(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| {
        app.group_call.stream.controls.hide_at().is_some()
    })
}

/// The stream window full screen and active, the mouse over the picture.
fn full_screen_under_the_mouse(app: &Entity<QuillApp>, call: &mut VisualTestContext) {
    call.update(|window, _| {
        window.activate_window();
        window.toggle_fullscreen();
    });
    app.update(call, |_, cx| cx.notify());
    frames(call);
    let viewport = find(call, "group-call-stream-viewport").expect("the picture");
    call.simulate_mouse_move(viewport.center(), None, Modifiers::default());
    frames(call);
}

/// Full screen the controls and the cursor hide after 5 idle seconds; a
/// move brings them back and starts the timer again; leaving full screen
/// shows them and stops it.
#[gpui_kit::test]
fn full_screen_hides_the_controls_and_the_cursor(cx: &mut TestAppContext) {
    let (app, _main, mut call) = open_stream(cx, StreamState::Live);
    let call = &mut call;
    full_screen_under_the_mouse(&app, call);
    assert!(
        controls_shown(&app, call),
        "entering full screen shows them"
    );
    let hides = cursor_hides();
    cx.executor().advance_clock(Duration::from_millis(4500));
    frames(call);
    assert!(controls_shown(&app, call), "still shown before 5 s");
    cx.executor().advance_clock(Duration::from_millis(700));
    frames(call);
    assert!(!controls_shown(&app, call), "hidden after 5 s");
    assert_eq!(cursor_hides(), hides + 1, "the cursor hides with them");
    let height = call.update(|window, _| window.viewport_size().height);
    cx.executor().advance_clock(Duration::from_millis(300));
    frames(call);
    let bar = find(call, "group-call-stream-bar").expect("the bar stays mounted");
    assert!(bar.top() >= height, "slid out of sight");
    // A move shows them again and re-arms the timer.
    let viewport = find(call, "group-call-stream-viewport").expect("the picture");
    call.simulate_mouse_move(
        viewport.center() + point(px(20.), px(10.)),
        None,
        Modifiers::default(),
    );
    frames(call);
    assert!(controls_shown(&app, call), "a move shows them");
    assert!(hide_pending(&app, call), "and starts the timer again");
    cx.executor().advance_clock(Duration::from_millis(5200));
    frames(call);
    assert!(!controls_shown(&app, call), "hidden again after 5 s");
    assert_eq!(cursor_hides(), hides + 2);
    // Leaving full screen with them hidden: shown (the mouse is over the
    // picture) and no timer.
    call.update(|window, _| window.toggle_fullscreen());
    app.update(call, |_, cx| cx.notify());
    frames(call);
    assert!(controls_shown(&app, call), "leaving full screen shows them");
    assert!(!hide_pending(&app, call), "and stops the timer");
    cx.executor().advance_clock(Duration::from_secs(6));
    frames(call);
    assert!(controls_shown(&app, call), "nothing hides them now");
    assert_eq!(cursor_hides(), hides + 2, "nor the cursor");
}

/// The platform hides the cursor system wide: with the pointer not over
/// the stream (another display, another app) the controls hide but the
/// cursor stays.
#[gpui_kit::test]
fn the_cursor_stays_when_the_pointer_is_elsewhere(cx: &mut TestAppContext) {
    let (app, _main, mut call) = open_stream(cx, StreamState::Live);
    let call = &mut call;
    call.update(|window, _| {
        window.activate_window();
        window.toggle_fullscreen();
    });
    app.update(call, |_, cx| cx.notify());
    frames(call);
    // The pointer off the window (another display, say).
    call.simulate_mouse_move(point(px(-40.), px(-40.)), None, Modifiers::default());
    frames(call);
    let hides = cursor_hides();
    cx.executor().advance_clock(Duration::from_millis(5200));
    frames(call);
    assert!(!controls_shown(&app, call), "the controls hide");
    assert_eq!(cursor_hides(), hides, "the cursor elsewhere stays");
}

/// Leaving the bar for the picture in full screen: tdesktop's Leave hides
/// the controls and the viewport's move shows them again, so they stay
/// and the cursor never flashes off.
#[gpui_kit::test]
fn leaving_the_bar_for_the_picture_keeps_the_controls_and_cursor(cx: &mut TestAppContext) {
    let (app, _main, mut call) = open_stream(cx, StreamState::Live);
    let call = &mut call;
    full_screen_under_the_mouse(&app, call);
    let bar = find(call, "group-call-stream-bar").expect("the bar");
    call.simulate_mouse_move(bar.center(), None, Modifiers::default());
    frames(call);
    assert!(controls_shown(&app, call));
    let hides = cursor_hides();
    let viewport = find(call, "group-call-stream-viewport").expect("the picture");
    call.simulate_mouse_move(
        point(viewport.center().x, viewport.top() + px(40.)),
        None,
        Modifiers::default(),
    );
    frames(call);
    assert!(
        controls_shown(&app, call),
        "the move over the picture shows them"
    );
    assert_eq!(cursor_hides(), hides, "the cursor stays");
}

/// Nobody broadcasting: tdesktop's no-stream text on the controls'
/// backdrop, with no spinner (nothing is on its way).
#[gpui_kit::test]
fn nobody_broadcasting_shows_the_no_stream_text(cx: &mut TestAppContext) {
    let (app, _main, mut call) = open_stream(cx, StreamState::NoStream);
    let call = &mut call;
    cx.executor().advance_clock(Duration::from_secs(1));
    frames(call);
    let text = app.update(call, |app, _| {
        let facts = app.group_call_stream_facts().expect("a stream");
        super::stream_label(&facts, "Quill News")
    });
    assert_eq!(
        text.as_deref(),
        Some(
            quill::lang::tr_with(
                quill::lang::keys::GROUP_CALL_NO_STREAM,
                &[("group", "Quill News")]
            )
            .as_str()
        )
    );
    assert!(find(call, "group-call-stream-label").is_some(), "the label");
    assert!(find(call, "live-spinner").is_none(), "no spinner");
}
