use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

fn frame(seq: u64) -> LiveFrame {
    LiveFrame::new(seq, 2, 2, LiveRotation::None, vec![0; 6], Instant::now()).expect("frame")
}

fn counter() -> (Arc<AtomicUsize>, impl Fn() -> LiveWaker) {
    let count = Arc::new(AtomicUsize::new(0));
    let make = {
        let count = count.clone();
        move || -> LiveWaker {
            let count = count.clone();
            Box::new(move || {
                count.fetch_add(1, Ordering::SeqCst);
            })
        }
    };
    (count, make)
}

#[test]
fn a_late_frame_replaces_one_not_shown_yet() {
    let (sink, source) = LiveSink::new();
    source.push(frame(1));
    source.push(frame(2));
    let snapshot = sink.snapshot();
    assert_eq!((snapshot.pushed, snapshot.dropped), (2, 1));
    assert_eq!(sink.take().map(|f| f.seq), Some(2));
    // Taken: nothing new until the next push, but the frame stays.
    assert!(sink.take().is_none());
    assert_eq!(sink.latest().map(|f| f.seq), Some(2));
    source.push(frame(3));
    assert_eq!(sink.snapshot().dropped, 1, "a shown frame is not dropped");
}

#[test]
fn a_push_wakes_the_armed_waker_once() {
    let (sink, source) = LiveSink::new();
    let (count, waker) = counter();
    sink.arm(waker());
    source.push(frame(1));
    source.push(frame(2));
    assert_eq!(count.load(Ordering::SeqCst), 1, "one wake per arm");
    // Pushed while unarmed: the next arm fires at once.
    sink.arm(waker());
    assert_eq!(count.load(Ordering::SeqCst), 2);
    // Nothing new since: the next arm waits.
    sink.arm(waker());
    assert_eq!(count.load(Ordering::SeqCst), 2);
}

#[test]
fn state_and_end_wake_and_later_frames_are_ignored() {
    let (sink, source) = LiveSink::new();
    let (count, waker) = counter();
    sink.arm(waker());
    source.set_state(LiveState::Paused);
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(sink.snapshot().state, Some(LiveState::Paused));
    // The same state again is no news.
    sink.arm(waker());
    source.set_state(LiveState::Paused);
    assert_eq!(count.load(Ordering::SeqCst), 1);
    source.end(LiveEnd::Over);
    assert_eq!(count.load(Ordering::SeqCst), 2);
    source.push(frame(1));
    assert!(sink.take().is_none());
    assert!(source.is_ended());
    assert_eq!(sink.snapshot().end, Some(LiveEnd::Over));
}

#[test]
fn a_frame_must_hold_its_planes() {
    let now = Instant::now();
    assert!(LiveFrame::new(1, 4, 2, LiveRotation::None, vec![0; 11], now).is_none());
    assert!(LiveFrame::new(1, 0, 2, LiveRotation::None, vec![], now).is_none());
    let turned = LiveFrame::new(1, 4, 2, LiveRotation::R90, vec![0; 12], now).expect("frame");
    assert_eq!(turned.shown_size(), (2, 4));
}

#[test]
fn the_source_is_send_and_the_sink_sees_it() {
    fn send<T: Send + Sync>(_: &T) {}
    let (sink, source) = LiveSink::new();
    send(&source);
    let other = source.sink();
    assert!(other.same_as(&sink));
    std::thread::spawn(move || source.push(frame(9)))
        .join()
        .expect("thread");
    assert_eq!(sink.take().map(|f| f.seq), Some(9));
}
