use super::super::backend_ffmpeg::NV12_RETRY;
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

/// A feed whose every frame converts during the pull.
fn inline_feed() -> (Rc<LiveCore>, LiveSource) {
    let (sink, source) = LiveSink::new();
    let mut core = LiveCore::new(sink, false);
    core.inline_always = true;
    (Rc::new(core), source)
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
    let (core, source) = inline_feed();
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
    core.landed(landed(&big, target));
    assert!(backend.take_picture(&present()).picture.is_some());
    // A small tile frame converts during the pull on macOS (NV12);
    // elsewhere (BGRA) it waits for the background too.
    source.push(frame(2, 320, 180, t0));
    if !live_picture::bgra_only() {
        assert!(core.take_background().is_none());
        assert!(backend.take_picture(&present()).picture.is_some());
    } else {
        assert!(backend.take_picture(&present()).picture.is_none());
        assert!(core.take_background().is_some());
    }
    assert_eq!(core.conversions.get(), 2);
}

fn landed(frame: &LiveFrame, target: Target) -> Option<Landed> {
    let prepared = live_picture::convert(frame, target, false, &mut Vec::new())?;
    Some(Landed {
        seq: frame.seq,
        prepared,
        convert_us: 0,
    })
}

fn bgra(seq: u64) -> Landed {
    Landed {
        seq,
        prepared: Prepared::Bgra {
            bgra: vec![seq as u8; 2 * 2 * 4],
            width: 2,
            height: 2,
        },
        convert_us: 0,
    }
}

#[test]
fn images_from_two_pictures_back_are_retired() {
    let (core, _source) = feed(false);
    for seq in 1..=4 {
        assert!(core.show(bgra(seq)).is_some());
    }
    // Shown and previous stay; the two before them leave.
    assert_eq!(core.take_retired().len(), 2);
    assert!(core.take_retired().is_empty());
    // The owner releases the rest as the player goes.
    assert_eq!(core.take_all_images().len(), 2);
}

#[test]
fn an_older_picture_never_replaces_a_newer_one() {
    let (core, _source) = feed(false);
    // A 1080p frame converts in the background while the sender drops
    // to a smaller size, which converts at once and is drawn first.
    assert!(core.show(bgra(5)).is_some());
    core.landed(Some(bgra(4)));
    assert!(core.take_ready().is_none(), "the late picture is dropped");
    assert!(core.show(bgra(3)).is_none(), "nor shown");
    // A newer one lands; an older one landing after it does not win.
    core.landed(Some(bgra(7)));
    core.landed(Some(bgra(6)));
    assert_eq!(core.take_ready().map(|ready| ready.seq), Some(7));
}

#[test]
fn the_pull_shows_the_newer_of_a_landed_and_an_inline_frame() {
    let (core, source) = inline_feed();
    let mut backend = LiveBackend { core: core.clone() };
    let t0 = Instant::now();
    core.landed(Some(bgra(1)));
    source.push(frame(2, 64, 36, t0));
    let picture = backend.take_picture(&present()).picture.expect("drawn");
    assert_eq!(picture.size(), (64, 36), "frame 2, not the landed 1");
    assert!(backend.take_picture(&present()).picture.is_none());
}

#[test]
fn a_size_change_mid_stream_shows_the_new_size() {
    let (core, source) = inline_feed();
    let mut backend = LiveBackend { core: core.clone() };
    let t0 = Instant::now();
    source.push(frame(1, 64, 36, t0));
    let first = backend.take_picture(&present()).picture.expect("first");
    assert_eq!(first.size(), (64, 36));
    source.push(frame(2, 128, 72, t0 + 33 * MS));
    let second = backend.take_picture(&present()).picture.expect("second");
    assert_eq!(second.size(), (128, 72));
    source.push(frame(3, 64, 36, t0 + 66 * MS));
    let third = backend.take_picture(&present()).picture.expect("third");
    assert_eq!(third.size(), (64, 36));
}

/// A core drawn in a view.
fn watched(tile: bool) -> (Rc<LiveCore>, LiveSource, EntityId) {
    let (core, source) = feed(tile);
    let view = EntityId::from(7u64);
    core.view.set(Some(view));
    (core, source, view)
}

fn paint(core: &LiveCore) {
    core.painted.set(core.painted.get() + 1);
}

#[test]
fn a_tile_painted_out_of_sight_stops_after_two_wakes() {
    let (core, _source, view) = watched(true);
    let t0 = Instant::now();
    // Painted in view (the count grows): every wake redraws.
    for (shown, at) in [(1, 0), (2, 33), (3, 66)] {
        paint(&core);
        assert_eq!(core.wake_view(shown, t0 + at * MS), Some(view));
    }
    // Painted off screen: one more redraw, then none...
    paint(&core);
    assert_eq!(core.wake_view(3, t0 + 100 * MS), Some(view));
    paint(&core);
    assert_eq!(core.wake_view(3, t0 + 133 * MS), None);
    assert!(core.take_inline().is_none(), "nothing converts meanwhile");
    assert_eq!(core.wake_view(3, t0 + 166 * MS), None);
    let asks = core.asks.get();
    // ...but a look once a second.
    let at = t0 + 133 * MS + UNSEEN_PROBE;
    assert_eq!(core.wake_view(3, at), Some(view));
    assert_eq!(core.asks.get(), asks + 1);
    // Back in view: every frame again.
    assert_eq!(core.wake_view(4, at + 33 * MS), Some(view));
    assert_eq!(core.wake_view(5, at + 66 * MS), Some(view));
}

#[test]
fn a_window_that_stopped_drawing_lets_its_pictures_go() {
    let (core, _source, view) = watched(false);
    let t0 = Instant::now();
    assert_eq!(core.wake_view(1, t0), Some(view));
    // No paint at all (minimized): redrawn for half a second...
    for at in [33, 66, 400, 532] {
        assert_eq!(core.wake_view(1, t0 + at * MS), Some(view), "{at} ms");
    }
    // ...then two wakes past it count, and it goes quiet...
    assert_eq!(core.wake_view(1, t0 + 566 * MS), Some(view));
    assert_eq!(core.wake_view(1, t0 + 600 * MS), None);
    assert_eq!(core.wake_view(1, t0 + 700 * MS), None);
    // ...looking four times a second, as a stream or a story.
    assert_eq!(core.wake_view(1, t0 + 850 * MS), Some(view));
}

#[test]
fn a_slow_window_keeps_its_pictures() {
    let (core, _source, view) = watched(true);
    let t0 = Instant::now();
    // 30 fps frames, the window drawing (in sight) every 150 ms.
    let mut shown = 0;
    for n in 0..60u32 {
        if n % 5 == 0 {
            shown += 1;
            paint(&core);
        }
        assert_eq!(
            core.wake_view(shown, t0 + n * 33 * MS),
            Some(view),
            "wake {n}"
        );
    }
}

#[test]
fn a_stream_window_drawn_in_sight_is_always_redrawn() {
    let (core, _source, view) = watched(false);
    let t0 = Instant::now();
    for n in 0..5u32 {
        paint(&core);
        assert_eq!(
            core.wake_view(u64::from(n) + 1, t0 + n * 500 * MS),
            Some(view)
        );
    }
}

#[test]
fn the_paused_picture_is_made_at_most_every_half_second() {
    let (core, source) = feed(true);
    assert!(core.paused_picture().is_none());
    let t0 = Instant::now();
    core.clock.set(Some(t0));
    source.push(frame(1, 320, 180, t0));
    let first = core.paused_picture().expect("picture");
    let size = first.size(0);
    assert_eq!((size.width.0, size.height.0), (90, 51));
    assert!(Arc::ptr_eq(&first, &core.paused_picture().expect("same")));
    // A new frame within half a second: the same picture.
    source.push(frame(2, 320, 180, t0));
    assert!(Arc::ptr_eq(&first, &core.paused_picture().expect("held")));
    // Past it: a new one; the old one stays as the previous.
    core.clock.set(Some(t0 + PAUSED_REFRESH));
    let second = core.paused_picture().expect("new");
    assert!(!Arc::ptr_eq(&first, &second));
    assert!(core.take_retired().is_empty());
    source.push(frame(3, 320, 180, t0));
    core.clock.set(Some(t0 + 2 * PAUSED_REFRESH));
    assert!(core.paused_picture().is_some());
    assert_eq!(core.take_retired().len(), 1, "the first goes");
}

#[test]
fn a_mirrored_feed_converts_mirrored() {
    let shot = frame(1, 8, 4, Instant::now());
    let picture =
        |mirrored| match live_picture::convert(&shot, Target::Bgra, mirrored, &mut Vec::new()) {
            Some(Prepared::Bgra { bgra, width, .. }) => (bgra, width),
            _ => panic!("a BGRA picture"),
        };
    let (plain, width) = picture(false);
    let (mirrored, _) = picture(true);
    let row = width as usize * 4;
    for (plain, mirrored) in plain.chunks_exact(row).zip(mirrored.chunks_exact(row)) {
        let mut reversed = plain.as_chunks::<4>().0.to_vec();
        reversed.reverse();
        assert_eq!(mirrored.as_chunks::<4>().0, reversed.as_slice());
    }
    assert_ne!(plain, mirrored);
}

#[test]
fn a_turned_feed_converts_through_one_buffer() {
    let (core, source) = inline_feed();
    let mut backend = LiveBackend { core: core.clone() };
    let t0 = Instant::now();
    let turned = |seq| {
        LiveFrame::new(
            seq,
            64,
            36,
            LiveRotation::R90,
            gradient_frame(64, 36, 0),
            t0,
        )
        .expect("frame")
    };
    source.push(turned(1));
    assert!(backend.take_picture(&present()).picture.is_some());
    let first = core.scratch.take();
    let held = first.as_ptr();
    assert!(!first.is_empty(), "the feed keeps its buffer");
    core.scratch.set(first);
    source.push(turned(2));
    assert!(backend.take_picture(&present()).picture.is_some());
    let second = core.scratch.take();
    assert_eq!(second.as_ptr(), held);
}

// Windows' NV12 path (`live_surface.rs`), forced on so it runs on every
// machine. Off Windows no renderer draws NV12 frames, so a frame shown
// there is its CPU image; the NV12 state, the pause and the buffers are
// the same code.

/// A feed on the NV12 path whose every frame converts during the pull.
fn nv12_feed() -> (Rc<LiveCore>, LiveSource) {
    let (core, source) = inline_feed();
    core.nv12.set(true);
    (core, source)
}

fn format(pulled: &Pulled) -> Option<&'static str> {
    pulled.stats.map(|stats| stats.format)
}

#[test]
fn nv12_frames_convert_during_the_pull_up_to_the_inline_edge() {
    let (core, source) = feed(false);
    core.nv12.set(true);
    let mut backend = LiveBackend { core: core.clone() };
    let t0 = Instant::now();
    source.push(frame(1, 320, 180, t0));
    let pulled = backend.take_picture(&present());
    assert!(pulled.picture.is_some(), "a small frame converts at once");
    let stats = pulled.stats.expect("a VF line");
    assert_eq!(
        (stats.format, stats.width, stats.height, stats.bytes),
        ("nv12", 320, 180, 320 * 180 * 3 / 2)
    );
    assert!(core.track.borrow().nv12.shown_frame().is_some());
    // A large one in the background, into a pool buffer.
    source.push(frame(2, 1280, 720, t0));
    assert!(backend.take_picture(&present()).picture.is_none());
    let (big, target) = core.take_background().expect("a background job");
    assert!(matches!(target, Target::Nv12(_)));
    core.landed(landed(&big, target));
    assert_eq!(format(&backend.take_picture(&present())), Some("nv12"));
    // An odd size packs without its last column and row.
    source.push(frame(3, 321, 181, t0));
    let pulled = backend.take_picture(&present());
    let stats = pulled.stats.expect("a VF line");
    assert_eq!(
        (stats.format, stats.width, stats.height),
        ("nv12", 320, 180)
    );
    // Too thin for a surface (1 px): BGRA, off the UI thread.
    source.push(frame(4, 1, 36, t0));
    assert!(backend.take_picture(&present()).picture.is_none());
    let (thin, target) = core.take_background().expect("a background job");
    assert!(matches!(target, Target::Bgra));
    core.landed(landed(&thin, target));
    assert_eq!(format(&backend.take_picture(&present())), Some("bgra"));
    assert!(core.track.borrow().nv12.shown_frame().is_none(), "an image");
}

#[test]
fn a_turned_mirrored_frame_packs_upright_into_nv12() {
    let (core, source) = nv12_feed();
    core.mirrored.set(true);
    let mut backend = LiveBackend { core: core.clone() };
    let planes = gradient_frame(64, 36, 0);
    let shot = LiveFrame::new(1, 64, 36, LiveRotation::R90, planes, Instant::now());
    source.push(shot.expect("frame"));
    let pulled = backend.take_picture(&present());
    assert_eq!(pulled.picture.map(|picture| picture.size()), Some((36, 64)));
    let frame = core.track.borrow().nv12.shown_frame().expect("nv12");
    assert_eq!((frame.width(), frame.height()), (36, 64));
    let (luma, _) = frame.nv12_planes().expect("planes");
    // Turned 90 clockwise (the left column becomes the top row), then
    // mirrored.
    let source_frame = LiveFrame::new(
        1,
        64,
        36,
        LiveRotation::None,
        gradient_frame(64, 36, 0),
        Instant::now(),
    )
    .expect("frame");
    let y = source_frame.i420().expect("planes").y;
    // (x, y) -> turned (35 - y, x) -> mirrored (y, x).
    for (sx, sy) in [(0usize, 0usize), (10, 3), (63, 35)] {
        assert_eq!(luma[sx * 36 + sy], y[sy * 64 + sx], "{sx},{sy}");
    }
}

#[test]
fn a_refused_frame_is_replaced_by_its_image_and_images_follow_for_the_pause() {
    let (core, source) = nv12_feed();
    let mut backend = LiveBackend { core: core.clone() };
    let t0 = Instant::now();
    core.clock.set(Some(t0));
    source.push(frame(1, 64, 36, t0));
    assert_eq!(format(&backend.take_picture(&present())), Some("nv12"));
    let shown = core.track.borrow().nv12.shown_frame().expect("nv12");
    // Not drawn yet: the element asks for the next display frame.
    assert_eq!(backend.next_due(t0, 16 * MS), None);
    shown.mark_drawn();
    assert_eq!(backend.next_due(t0, 16 * MS), Some(t0 + STALL_AFTER));
    shown.mark_refused();
    assert_eq!(backend.next_due(t0, 16 * MS), None);
    let replaced = backend
        .replace_refused(t0)
        .expect("the picture as an image");
    assert!(matches!(replaced, Picture::Image(_)));
    assert_eq!(replaced.size(), (64, 36));
    assert!(core.track.borrow().nv12.shown_frame().is_none());
    // Settled: back to the stall check.
    assert_eq!(backend.next_due(t0, 16 * MS), Some(t0 + STALL_AFTER));
    // Images for the file player's pause...
    for (seq, at) in [(2u64, 33u32), (3, 1999)] {
        core.clock.set(Some(t0 + at * MS));
        source.push(frame(seq, 64, 36, t0 + at * MS));
        let pulled = backend.take_picture(&present());
        assert_eq!(format(&pulled), Some("bgra"), "{at} ms");
    }
    // ...then NV12 again.
    core.clock.set(Some(t0 + NV12_RETRY));
    source.push(frame(4, 64, 36, t0 + NV12_RETRY));
    assert_eq!(format(&backend.take_picture(&present())), Some("nv12"));
    // Every image of the pause leaves the atlas: the refused frame's and
    // the two BGRA frames. On Windows the NV12 frame after them retires
    // both kept images; elsewhere NV12 frames are images themselves.
    let retired = core.take_retired().len();
    let kept = core.take_all_images().len();
    if cfg!(target_os = "windows") {
        assert_eq!((retired, kept), (3, 0));
    } else {
        assert_eq!((retired, kept), (3, 2));
    }
}

#[test]
fn nv12_buffers_are_reused_and_go_with_the_feed_and_the_player() {
    let (core, source) = nv12_feed();
    let mut backend = LiveBackend { core: core.clone() };
    let t0 = Instant::now();
    // Where each frame's pixels live (a weak handle would stop the reuse:
    // a buffer is filled again only when nothing else refers to it).
    let mut distinct: Vec<*const u8> = Vec::new();
    for seq in 1..=30u64 {
        source.push(frame(seq, 64, 36, t0 + seq as u32 * 33 * MS));
        assert!(backend.take_picture(&present()).picture.is_some());
        let shown = core.track.borrow().nv12.shown_frame().expect("nv12");
        distinct.push(shown.bytes().as_ptr());
    }
    distinct.sort_unstable();
    distinct.dedup();
    // The frame on screen and the one before it: two buffers in turn.
    assert!(
        distinct.len() <= 3,
        "{} buffers for 30 frames",
        distinct.len()
    );
    let buffers = core.track.borrow().nv12.pooled();
    assert!(!buffers.is_empty());
    // The feed ends: the pool goes, the picture on screen stays.
    core.release_buffers(false);
    assert!(core.track.borrow().nv12.pooled().is_empty());
    assert!(core.track.borrow().nv12.shown_frame().is_some());
    // The player goes.
    core.release_buffers(true);
    assert!(core.track.borrow().nv12.shown_frame().is_none());
    drop(backend);
    drop(core);
    assert!(
        buffers.iter().all(|bytes| bytes.strong_count() == 0),
        "every buffer freed"
    );
}
