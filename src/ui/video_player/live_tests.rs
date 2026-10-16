use super::super::transport::{LiveStatus, TransportMachine};
use super::super::{PlayState, Present};
use super::*;
// `gpui_kit::*` above brings its own `test` attribute; these are plain.
use core::prelude::v1::test;
use quill::calls::live_video::convert::gradient_frame;
use quill::calls::live_video::{LiveRotation, LiveSource};

const MS: Duration = Duration::from_millis(1);

fn frame(seq: u64, width: u16, height: u16, at: Instant) -> LiveFrame {
    LiveFrame::new(
        seq,
        width,
        height,
        LiveRotation::None,
        gradient_frame(width, height, 0),
        at,
    )
    .expect("frame")
}

fn feed(tile: bool) -> (Rc<LiveCore>, LiveSource) {
    let (sink, source) = LiveSink::new();
    (Rc::new(LiveCore::new(sink, tile)), source)
}

fn present() -> Present {
    Present {
        at: Instant::now(),
        #[cfg(target_os = "macos")]
        host_time: None,
    }
}

#[test]
fn the_median_gap_takes_the_last_eight() {
    let t0 = Instant::now();
    let mut gaps = ArrivalGaps::default();
    assert_eq!(gaps.median(), None);
    gaps.note(1, t0);
    // The same frame again is no gap.
    gaps.note(1, t0 + 5 * MS);
    assert_eq!(gaps.median(), None);
    let mut at = t0;
    for (seq, gap) in [33u32, 34, 33, 200, 33, 32, 34, 33].iter().enumerate() {
        at += *gap * MS;
        gaps.note(seq as u64 + 2, at);
    }
    // One late frame does not move the median.
    assert_eq!(gaps.median(), Some(33 * MS));
    // Eight slow gaps push the fast ones out.
    for seq in 10..18 {
        at += 1000 * MS;
        gaps.note(seq, at);
    }
    assert_eq!(gaps.median(), Some(1000 * MS));
}

#[test]
fn the_stall_time_is_one_and_a_half_seconds_or_four_gaps() {
    let t0 = Instant::now();
    let mut gaps = ArrivalGaps::default();
    assert_eq!(gaps.stall_after(), STALL_AFTER);
    gaps.note(1, t0);
    gaps.note(2, t0 + 33 * MS);
    assert_eq!(gaps.stall_after(), STALL_AFTER);
    // A screen share at one frame a second stalls after four.
    let mut slow = ArrivalGaps::default();
    for seq in 0..4 {
        slow.note(seq, t0 + seq as u32 * 1000 * MS);
    }
    assert_eq!(slow.stall_after(), 4000 * MS);
}

fn snapshot(latest: Option<Instant>, state: Option<LiveState>) -> LiveSnapshot {
    LiveSnapshot {
        state,
        latest: latest.map(|at| (1, at)),
        ..LiveSnapshot::default()
    }
}

#[test]
fn the_status_follows_first_frame_stall_pause_and_end() {
    let t0 = Instant::now();
    let gaps = ArrivalGaps::default();
    let mut machine = TransportMachine::live(t0);
    assert_eq!(machine.state(), PlayState::Opening);
    assert!(machine.snapshot(t0).live);
    // No frame yet: still opening, nothing to check.
    let waiting = live_status(&snapshot(None, Some(LiveState::Active)), &gaps, t0);
    machine.observe_live(&waiting, t0);
    assert_eq!(machine.state(), PlayState::Opening);
    assert_eq!(stall_check_at(&waiting, None, &gaps, t0), None);
    // The first frame: playing, looked at again when it would stall.
    let playing = live_status(&snapshot(Some(t0), None), &gaps, t0);
    machine.observe_live(&playing, t0);
    assert_eq!(machine.state(), PlayState::Playing);
    assert_eq!(
        stall_check_at(&playing, Some(t0), &gaps, t0),
        Some(t0 + STALL_AFTER)
    );
    // Just short of the stall time: still playing.
    let later = t0 + STALL_AFTER.saturating_sub(MS);
    machine.observe_live(&live_status(&snapshot(Some(t0), None), &gaps, later), later);
    assert_eq!(machine.state(), PlayState::Playing);
    // 1.5 s without a frame: buffering, looked at every 250 ms.
    let stalled_at = t0 + STALL_AFTER;
    let stalled = live_status(&snapshot(Some(t0), None), &gaps, stalled_at);
    assert!(stalled.stalled);
    machine.observe_live(&stalled, stalled_at);
    assert_eq!(machine.state(), PlayState::Buffering);
    assert_eq!(
        stall_check_at(&stalled, Some(t0), &gaps, stalled_at),
        Some(stalled_at + 250 * MS)
    );
    // The sender paused: paused, not buffering, nothing to check.
    let paused = live_status(
        &snapshot(Some(t0), Some(LiveState::Paused)),
        &gaps,
        stalled_at,
    );
    assert!(!paused.stalled);
    machine.observe_live(&paused, stalled_at);
    assert_eq!(machine.state(), PlayState::Paused);
    assert_eq!(stall_check_at(&paused, Some(t0), &gaps, stalled_at), None);
    // No video from the sender: ready, not opening.
    machine.observe_live(
        &live_status(
            &snapshot(Some(t0), Some(LiveState::Inactive)),
            &gaps,
            stalled_at,
        ),
        stalled_at,
    );
    assert_eq!(machine.state(), PlayState::Ready);
    // A new frame: playing again.
    let resumed = live_status(&snapshot(Some(stalled_at), None), &gaps, stalled_at);
    machine.observe_live(&resumed, stalled_at);
    assert_eq!(machine.state(), PlayState::Playing);
    // The end, whatever the sender did.
    let over = LiveSnapshot {
        end: Some(LiveEnd::Over),
        ..snapshot(Some(t0), Some(LiveState::Paused))
    };
    let ended = live_status(&over, &gaps, stalled_at);
    machine.observe_live(&ended, stalled_at);
    assert_eq!(machine.state(), PlayState::Ended);
    assert_eq!(stall_check_at(&ended, Some(t0), &gaps, stalled_at), None);
}

#[test]
fn an_engine_failure_fails_for_good_and_live_has_no_transport_to_drive() {
    let t0 = Instant::now();
    let mut machine = TransportMachine::live(t0);
    let failed = LiveStatus {
        ended: Some(true),
        ..LiveStatus::default()
    };
    machine.observe_live(&failed, t0);
    assert_eq!(machine.state(), PlayState::Failed);
    machine.observe_live(
        &LiveStatus {
            has_frame: true,
            ..LiveStatus::default()
        },
        t0,
    );
    assert_eq!(machine.state(), PlayState::Failed);
    // Play, pause, seek and rate do nothing.
    let mut machine = TransportMachine::live(t0);
    machine.observe_live(
        &LiveStatus {
            has_frame: true,
            ..LiveStatus::default()
        },
        t0,
    );
    machine.pause(t0);
    assert_eq!(machine.state(), PlayState::Playing);
    assert_eq!(machine.seek(30.0, t0), 0.0);
    assert_eq!(machine.set_rate(2.0, t0), 1.0);
    let transport = machine.snapshot(t0 + Duration::from_secs(5));
    assert_eq!((transport.position, transport.duration), (0.0, None));
    assert_eq!(transport.rate, 1.0);
}

#[test]
fn a_late_frame_replaces_one_not_drawn_yet() {
    let (core, source) = feed(false);
    let mut backend = LiveBackend { core: core.clone() };
    let t0 = Instant::now();
    source.push(frame(1, 64, 36, t0));
    source.push(frame(2, 64, 36, t0 + 33 * MS));
    let pulled = backend.take_picture(&present());
    assert!(pulled.picture.is_some());
    // The first was never drawn; nothing new until the next push.
    assert_eq!(core.sink.snapshot().dropped, 1);
    assert!(backend.take_picture(&present()).picture.is_none());
}

#[test]
fn large_frames_wait_for_the_background_and_small_ones_convert_at_once() {
    let (core, source) = feed(false);
    let mut backend = LiveBackend { core: core.clone() };
    let t0 = Instant::now();
    source.push(frame(1, 1280, 720, t0));
    // Not on the UI thread...
    assert!(backend.take_picture(&present()).picture.is_none());
    // ...but for the waker task, which lands it for the next pull.
    let (big, target) = core.take_background().expect("a background job");
    assert_eq!(big.width, 1280);
    core.landed(live_picture::convert(&big, target));
    assert!(backend.take_picture(&present()).picture.is_some());
    // A small tile frame converts during the pull.
    source.push(frame(2, 320, 180, t0));
    assert!(core.take_background().is_none());
    assert!(backend.take_picture(&present()).picture.is_some());
}

#[test]
fn images_from_two_pictures_back_are_retired() {
    let (core, _source) = feed(false);
    let image = |n: u8| Prepared::Bgra {
        bgra: vec![n; 2 * 2 * 4],
        width: 2,
        height: 2,
    };
    for n in 0..4 {
        core.show(image(n));
    }
    // Shown and previous stay; the two before them leave.
    assert_eq!(core.take_retired().len(), 2);
    assert!(core.take_retired().is_empty());
}

#[test]
fn an_off_screen_tile_stops_redrawing_after_two_wakes() {
    let (core, _source) = feed(true);
    let view = EntityId::from(7u64);
    let t0 = Instant::now();
    // Not drawn yet: nothing to redraw.
    assert_eq!(core.wake_view(0, t0), None);
    core.view.set(Some(view));
    // Painted in view (the count grows): every wake redraws.
    for (shown, at) in [(1, 0), (2, 33), (3, 66)] {
        assert_eq!(core.wake_view(shown, t0 + at * MS), Some(view));
    }
    // Off screen: one more redraw, then none...
    let mut at = t0 + 200 * MS;
    assert_eq!(core.wake_view(3, at), Some(view));
    at += 200 * MS;
    assert_eq!(core.wake_view(3, at), None);
    at += 200 * MS;
    assert_eq!(core.wake_view(3, at), None);
    let asks = core.asks.get();
    // ...but a look once a second.
    at += UNSEEN_PROBE;
    assert_eq!(core.wake_view(3, at), Some(view));
    assert_eq!(core.asks.get(), asks + 1);
    // Back in view: every frame again.
    assert_eq!(core.wake_view(4, at + 33 * MS), Some(view));
    assert_eq!(core.wake_view(5, at + 66 * MS), Some(view));
}

#[test]
fn wakes_close_together_do_not_count_as_unseen() {
    let (core, _source) = feed(true);
    let view = EntityId::from(7u64);
    core.view.set(Some(view));
    let t0 = Instant::now();
    assert_eq!(core.wake_view(1, t0), Some(view));
    // Frames faster than the window draws: still redrawn.
    for at in [5, 10, 15, 20] {
        assert_eq!(core.wake_view(1, t0 + at * MS), Some(view));
    }
}

#[test]
fn a_stream_window_is_always_redrawn() {
    let (core, _source) = feed(false);
    let view = EntityId::from(9u64);
    core.view.set(Some(view));
    let t0 = Instant::now();
    for n in 0..5 {
        assert_eq!(core.wake_view(0, t0 + n * 500 * MS), Some(view));
    }
}

#[test]
fn the_paused_picture_is_made_once_per_frame() {
    let (core, source) = feed(true);
    assert!(core.paused_picture().is_none());
    let t0 = Instant::now();
    source.push(frame(1, 320, 180, t0));
    let first = core.paused_picture().expect("picture");
    let size = first.size(0);
    assert_eq!((size.width.0, size.height.0), (90, 51));
    assert!(Arc::ptr_eq(&first, &core.paused_picture().expect("same")));
    source.push(frame(2, 320, 180, t0));
    assert!(!Arc::ptr_eq(&first, &core.paused_picture().expect("new")));
    assert_eq!(core.take_retired().len(), 1);
}
