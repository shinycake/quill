//! The player over a scripted backend: pulls, the frame demand, loops,
//! frame steps and the watcher cadence; and (with `demo-capture`) the
//! viewer opening a clip and closing while it still opens.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use super::transport::BackendStatus;
use super::{
    Backend, ErrorKind, Picture, PlayState, PlayerError, Present, Pulled, VideoPlayer,
    WATCH_INTERVAL, trace_line,
};
use gpui_kit::{EntityId, RenderImage};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

thread_local! {
    /// The next `VideoPlayer::open` on this thread gets this backend.
    static NEXT_BACKEND: RefCell<Option<Box<dyn Backend>>> = const { RefCell::new(None) };
}

/// Take the scripted backend a test installed, if any (`open_backend`).
pub(super) fn take_scripted_backend() -> Option<Box<dyn Backend>> {
    NEXT_BACKEND.with(|next| next.borrow_mut().take())
}

#[derive(Default)]
struct Script {
    ready: bool,
    position: f64,
    at_end: bool,
    /// Every call, in order.
    calls: Vec<String>,
    /// The present time of every pull.
    presents: Vec<Instant>,
    /// A picture to hand out on the next pull.
    picture: bool,
    next_due: Option<Instant>,
    /// The buffer ran dry while playing.
    stalled: bool,
    /// The decoder failed.
    error: Option<PlayerError>,
    dropped: bool,
}

struct Scripted(Rc<RefCell<Script>>);

impl Drop for Scripted {
    fn drop(&mut self) {
        self.0.borrow_mut().dropped = true;
    }
}

fn image() -> Arc<RenderImage> {
    let pixels = image::RgbaImage::from_raw(1, 1, vec![0, 0, 0, 255]).unwrap();
    Arc::new(RenderImage::new(smallvec::SmallVec::from_buf([
        image::Frame::new(pixels),
    ])))
}

impl Backend for Scripted {
    fn status(&mut self) -> BackendStatus {
        let script = self.0.borrow();
        BackendStatus {
            ready: script.ready,
            position: script.position,
            duration: Some(10.0),
            at_end: script.at_end,
            stalled: script.stalled,
            failed: script.error.is_some(),
            buffered: vec![0.0..10.0],
            has_sound: true,
        }
    }
    fn play(&mut self, rate: f64) {
        self.0.borrow_mut().calls.push(format!("play {rate}"));
    }
    fn pause(&mut self) {
        self.0.borrow_mut().calls.push("pause".into());
    }
    fn seek(&mut self, secs: f64) {
        let mut script = self.0.borrow_mut();
        script.calls.push(format!("seek {secs}"));
        script.position = secs;
        script.at_end = false;
    }
    fn set_volume(&mut self, volume: f32) {
        self.0.borrow_mut().calls.push(format!("volume {volume}"));
    }
    fn set_rate(&mut self, rate: f64, playing: bool) {
        self.0
            .borrow_mut()
            .calls
            .push(format!("rate {rate} {playing}"));
    }
    fn step_frame(&mut self, forward: bool) {
        self.0.borrow_mut().calls.push(format!("step {forward}"));
    }
    fn take_picture(&mut self, present: &Present) -> Pulled {
        let mut script = self.0.borrow_mut();
        script.presents.push(present.at);
        let picture = std::mem::take(&mut script.picture).then(|| Picture::Image(image()));
        Pulled {
            display_secs: picture.as_ref().map(|_| script.position),
            picture,
            item_secs: script.position,
        }
    }
    fn next_due(&self, _now: Instant, _interval: Duration) -> Option<Instant> {
        self.0.borrow().next_due
    }
    fn error(&self) -> Option<PlayerError> {
        self.0.borrow().error.clone()
    }
}

fn scripted(ready: bool) -> (VideoPlayer, Rc<RefCell<Script>>) {
    let script = Rc::new(RefCell::new(Script {
        ready,
        ..Script::default()
    }));
    let player = VideoPlayer::from_backend(Box::new(Scripted(script.clone())), Some(10.0));
    (player, script)
}

fn calls(script: &Rc<RefCell<Script>>) -> Vec<String> {
    std::mem::take(&mut script.borrow_mut().calls)
}

#[test]
fn play_waits_for_the_open_then_starts_the_backend() {
    let (player, script) = scripted(false);
    player.play();
    assert_eq!(player.state(), PlayState::Opening);
    assert!(calls(&script).is_empty());
    // Opening wants pictures every frame, until the first one comes.
    assert!(player.frame_demand(Instant::now()).animates);
    script.borrow_mut().ready = true;
    player.pull(Instant::now(), None);
    assert_eq!(player.state(), PlayState::Playing);
    assert_eq!(calls(&script), ["play 1"]);
}

#[test]
fn a_pull_asks_for_the_picture_at_the_predicted_present_time() {
    let (player, script) = scripted(true);
    player.play();
    let t0 = Instant::now();
    let interval = Duration::from_micros(16_667);
    for i in 0..6 {
        script.borrow_mut().picture = i % 2 == 0;
        let shown = player.pull(t0 + interval * i, None);
        assert!(shown.is_some(), "the last picture stays on screen");
    }
    let presents = script.borrow().presents.clone();
    assert_eq!(presents.len(), 6);
    // Each pull asks for the vsync after its draw started, not the draw
    // time itself.
    for (i, present) in presents.iter().enumerate().skip(2) {
        let drawn = t0 + interval * i as u32;
        let ahead = present.saturating_duration_since(drawn);
        assert!(
            ahead > Duration::ZERO && ahead <= interval + Duration::from_millis(1),
            "pull {i}: {ahead:?}"
        );
    }
    // With the platform's present time for this frame, that one is used.
    let hint = t0 + interval * 7;
    player.pull(t0 + interval * 6, Some((hint, 42)));
    assert_eq!(script.borrow().presents.last().copied(), Some(hint));
}

#[test]
fn a_paused_clip_stops_asking_for_frames_once_its_picture_came() {
    let (player, script) = scripted(true);
    player.pull(Instant::now(), None);
    assert_eq!(player.state(), PlayState::Ready);
    // The first picture is still awaited.
    assert!(player.frame_demand(Instant::now()).animates);
    script.borrow_mut().picture = true;
    player.pull(Instant::now(), None);
    assert!(!player.frame_demand(Instant::now()).animates);
    // A seek wants the picture there.
    player.seek(4.0);
    assert!(player.frame_demand(Instant::now()).animates);
    script.borrow_mut().picture = true;
    player.pull(Instant::now(), None);
    assert!(!player.frame_demand(Instant::now()).animates);
    // Playing asks every frame, with the backend's due time.
    let due = Instant::now() + Duration::from_millis(30);
    script.borrow_mut().next_due = Some(due);
    player.play();
    let demand = player.frame_demand(Instant::now());
    assert!(demand.animates);
    assert_eq!(demand.next_due, Some(due));
    assert!(demand.plays_sound);
}

#[test]
fn the_transport_loops_at_the_end_and_steps_only_while_paused() {
    let (player, script) = scripted(true);
    player.set_looping(true);
    player.play();
    player.pull(Instant::now(), None);
    calls(&script);
    player.step_frame(true);
    assert!(calls(&script).is_empty(), "no steps while playing");
    {
        let mut script = script.borrow_mut();
        script.position = 10.0;
        script.at_end = true;
    }
    player.pull(Instant::now(), None);
    assert_eq!(calls(&script), ["seek 0", "play 1"]);
    assert_eq!(player.state(), PlayState::Playing);
    player.pause();
    player.step_frame(true);
    player.step_frame(false);
    assert_eq!(calls(&script), ["pause", "step true", "step false"]);
}

#[test]
fn watchers_redraw_on_changes_and_at_most_every_100_ms_while_playing() {
    let (player, script) = scripted(true);
    let watcher = EntityId::from(7u64);
    player.watch(watcher);
    player.watch(watcher);
    let t0 = Instant::now();
    assert_eq!(player.due_watchers(t0), [watcher]);
    assert!(player.due_watchers(t0).is_empty());
    player.play();
    player.pull(t0, None);
    // Ready → Playing.
    assert_eq!(player.due_watchers(t0), [watcher]);
    assert!(player.due_watchers(t0 + WATCH_INTERVAL / 2).is_empty());
    assert_eq!(player.due_watchers(t0 + WATCH_INTERVAL), [watcher]);
    assert_eq!(calls(&script), ["play 1"]);
}

#[test]
fn a_stall_buffers_and_playback_resumes_when_the_buffer_catches_up() {
    let (player, script) = scripted(true);
    player.play();
    player.pull(Instant::now(), None);
    assert_eq!(player.state(), PlayState::Playing);
    script.borrow_mut().stalled = true;
    player.pull(Instant::now(), None);
    assert_eq!(player.state(), PlayState::Buffering);
    // Buffering still draws (the spinner, the picture once it comes).
    assert!(player.frame_demand(Instant::now()).animates);
    script.borrow_mut().stalled = false;
    player.pull(Instant::now(), None);
    assert_eq!(player.state(), PlayState::Playing);
}

#[test]
fn a_failing_backend_fails_the_transport_and_keeps_the_last_picture() {
    let (player, script) = scripted(true);
    player.play();
    script.borrow_mut().picture = true;
    let shown = player.pull(Instant::now(), None);
    assert!(shown.is_some());
    script.borrow_mut().error = Some(PlayerError::new(ErrorKind::Decode, "broken"));
    let kept = player.pull(Instant::now(), None);
    assert_eq!(player.state(), PlayState::Failed);
    assert!(kept.is_some(), "the last picture stays on screen");
    let error = player.take_error();
    assert_eq!(error.map(|err| err.kind), Some(ErrorKind::Decode));
    assert!(player.take_error().is_none(), "reported once");
    assert!(!player.frame_demand(Instant::now()).animates);
    // Still failed (and quiet) on the next draw.
    assert!(player.pull(Instant::now(), None).is_some());
    assert!(player.take_error().is_none());
}

#[test]
fn dropping_the_last_handle_releases_the_backend() {
    let (player, script) = scripted(true);
    let other = player.clone();
    drop(player);
    assert!(!script.borrow().dropped);
    drop(other);
    assert!(script.borrow().dropped);
}

#[test]
fn the_trace_line_has_the_documented_columns() {
    let line = trace_line(7, Instant::now(), 1.25, true, Some(1.2), 1.5);
    let columns: Vec<&str> = line.split(',').collect();
    assert_eq!(columns.len(), 7, "{line}");
    assert_eq!(columns[0], "VT");
    assert_eq!(columns[1], "7");
    assert_eq!(columns[3], "1250.000");
    assert_eq!(columns[4], "1");
    assert_eq!(columns[5], "1200.000");
    assert_eq!(columns[6], "1.50");
    let none = trace_line(7, Instant::now(), 0.0, false, None, 0.0);
    assert!(none.ends_with(",0,-1000.000,0.00"), "{none}");
}

/// The viewer on a clip that is still opening: the transport shows it,
/// and closing then drops the player without a crash.
#[cfg(feature = "demo-capture")]
mod viewer {
    use super::{NEXT_BACKEND, Script, Scripted};
    use crate::ui::app::QuillApp;
    use crate::ui::keybindings::bind_keys;
    use crate::ui::screenshot_demo::ScreenshotDemo;
    use crate::ui::shell::QuillShell;
    use crate::ui::video_player::PlayState;
    use gpui_kit::component::Root;
    use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
    use quill::ids::{ChatId, MessageId};
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::atomic::Ordering;

    fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
        cx.update(gpui_kit::init);
        cx.update(bind_keys);
        let slot = Rc::new(RefCell::new(None));
        let slot_in = slot.clone();
        let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
            let view = cx.new(|cx| {
                QuillApp::new_with_demo(
                    window,
                    cx,
                    None,
                    Some(ScreenshotDemo::named("ready-privacy")),
                )
            });
            let shell = cx.new(|_| QuillShell::new(view.clone()));
            *slot_in.borrow_mut() = Some(view);
            Root::new(shell, window, cx)
        });
        let app = slot.borrow().clone().unwrap();
        let vcx = VisualTestContext::from_window(handle.into(), cx);
        (app, vcx)
    }

    fn settle(vcx: &mut VisualTestContext) {
        for _ in 0..3 {
            vcx.update(|window, _| window.refresh());
            vcx.run_until_parked();
        }
    }

    #[gpui_kit::test]
    fn the_viewer_shows_an_opening_clip_and_closes_while_it_opens(cx: &mut TestAppContext) {
        let (app, mut vcx) = new_app(cx);
        let vcx = &mut vcx;
        app.update_in(vcx, |app, _, cx| {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
                crate::ui::inline_playback::apply_ready_video_viewer(
                    session,
                    &app.demo_ui.sink,
                    &app.demo_ui.seq,
                );
            }
            cx.notify();
        });
        let script = Rc::new(RefCell::new(Script::default()));
        NEXT_BACKEND.with(|next| *next.borrow_mut() = Some(Box::new(Scripted(script.clone()))));
        app.update_in(vcx, |app, _, cx| {
            app.open_media_viewer(ChatId(11), MessageId(204), cx);
        });
        settle(vcx);
        app.read_with(vcx, |app, _| {
            assert!(app.viewer.state.is_open());
            let transport = app.viewer_transport().expect("a player for the clip");
            assert_eq!(transport.state, PlayState::Opening);
            assert_eq!(transport.duration, Some(10.0));
            assert!(app.viewer.video_layer.is_some(), "the video layer is drawn");
        });
        app.update_in(vcx, |app, _, cx| app.close_media_viewer(cx));
        settle(vcx);
        app.read_with(vcx, |app, _| {
            assert!(!app.viewer.state.is_open());
            assert!(app.viewer.player.is_none());
        });
        assert!(script.borrow().dropped, "closing released the backend");
    }

    /// Space through the whole window: the key-down reaches the viewer's
    /// key handler and the key-up `QuillApp`'s root capture listener, so a
    /// tap toggles pause and a hold plays at 2x until release.
    #[gpui_kit::test]
    fn space_taps_toggle_and_holds_boost(cx: &mut TestAppContext) {
        use gpui_kit::{Focusable as _, KeyUpEvent, Keystroke};
        use quill::media_viewer::SPEED_BOOST_HOLD_MS;
        use std::time::Duration;

        let (app, mut vcx) = new_app(cx);
        let vcx = &mut vcx;
        // A background window ends holds (its key-up goes elsewhere).
        vcx.update(|window, _| window.activate_window());
        app.update_in(vcx, |app, window, cx| {
            if let Some(session) = app.demo_session.as_mut() {
                app.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
                crate::ui::inline_playback::apply_ready_video_viewer(
                    session,
                    &app.demo_ui.sink,
                    &app.demo_ui.seq,
                );
            }
            let focus = app.focus_handle(cx);
            window.focus(&focus, cx);
            cx.notify();
        });
        let script = Rc::new(RefCell::new(Script {
            ready: true,
            ..Script::default()
        }));
        NEXT_BACKEND.with(|next| *next.borrow_mut() = Some(Box::new(Scripted(script.clone()))));
        app.update_in(vcx, |app, _, cx| {
            app.open_media_viewer(ChatId(11), MessageId(204), cx);
        });
        settle(vcx);
        let state = |vcx: &mut VisualTestContext| {
            app.read_with(vcx, |app, _| {
                (
                    app.viewer_video_playing(),
                    app.viewer.controls.boost.is_active(),
                )
            })
        };
        let space_up = |vcx: &mut VisualTestContext| {
            vcx.simulate_event(KeyUpEvent {
                keystroke: Keystroke::parse("space").unwrap(),
            });
            settle(vcx);
        };
        assert_eq!(state(vcx), (true, false), "the clip autoplays");
        script.borrow_mut().calls.clear();

        // A tap: nothing on the press, pause on the release.
        vcx.simulate_keystrokes("space");
        assert_eq!(state(vcx), (true, false));
        space_up(vcx);
        assert_eq!(state(vcx), (false, false));
        assert!(script.borrow().calls.contains(&"pause".to_string()));
        script.borrow_mut().calls.clear();

        // A hold: 2x after the delay (even while paused, like tdesktop),
        // back to the chosen rate on release, and no toggle.
        vcx.simulate_keystrokes("space");
        vcx.executor()
            .advance_clock(Duration::from_millis(SPEED_BOOST_HOLD_MS + 50));
        settle(vcx);
        assert_eq!(state(vcx), (false, true));
        space_up(vcx);
        assert_eq!(state(vcx), (false, false));
        let calls = std::mem::take(&mut script.borrow_mut().calls);
        assert!(
            calls.iter().any(|c| c.starts_with("rate 2 ")),
            "boosted: {calls:?}"
        );
        assert!(
            calls.last().is_some_and(|c| c.starts_with("rate 1 ")),
            "restored: {calls:?}"
        );
        assert!(!calls.iter().any(|c| c.starts_with("play")), "{calls:?}");

        // Paused: `.` and `,` step one frame.
        vcx.simulate_keystrokes(". ,");
        let calls = std::mem::take(&mut script.borrow_mut().calls);
        assert_eq!(calls, ["step true", "step false"]);
    }
}
