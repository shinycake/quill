use super::{Appearing, Frame};
use crate::ui::text_reveal::{RevealCut, RevealLine};
use std::time::{Duration, Instant};

fn line(index: usize, width: f32) -> RevealLine {
    RevealLine {
        top: index as f32 * 20.,
        bottom: (index + 1) as f32 * 20.,
        width,
        rtl: false,
    }
}

fn at(t0: Instant, ms: u64) -> Instant {
    t0 + Duration::from_millis(ms)
}

fn revealed(frame: Frame) -> Option<(f32, f32)> {
    match frame.cut {
        RevealCut::Line { line, revealed } => Some((line.top, revealed)),
        RevealCut::All => None,
    }
}

#[test]
fn first_frame_without_lines_shows_nothing_and_asks_for_the_next() {
    let t0 = Instant::now();
    let mut appearing = Appearing::new();
    let frame = appearing.frame(Vec::new(), 0., t0);
    assert_eq!(frame.height, 0.);
    assert!(frame.moving, "the lines arrive with the next frame");
}

#[test]
fn a_last_line_sweeps_in_300_ms_and_the_first_sweep_at_least_160() {
    let t0 = Instant::now();
    let mut appearing = Appearing::new();
    let lines = vec![line(0, 100.)];
    let first = appearing.frame(lines.clone(), 0., t0);
    assert_eq!(revealed(first), Some((0., 0.)));
    assert_eq!(first.height, 20., "the bubble opens to the first line");
    let mid = appearing.frame(lines.clone(), 0., at(t0, 150));
    let (_, width) = revealed(mid).expect("still sweeping");
    assert!((width - 50.).abs() < 0.5, "{width}");
    let done = appearing.frame(lines, 0., at(t0, 300));
    assert_eq!(done.cut, RevealCut::All);
    assert!(!done.moving);
}

#[test]
fn a_new_line_opens_before_it_sweeps() {
    let t0 = Instant::now();
    let mut appearing = Appearing::new();
    appearing.frame(vec![line(0, 430.)], 0., t0);
    // Now caught up on the first line, more text wraps to a second one.
    appearing.frame(vec![line(0, 430.)], 0., at(t0, 300));
    appearing.text_changed(false);
    let lines = vec![line(0, 430.), line(1, 200.)];
    // The first line is done: the bubble opens to the second (100 ms)
    // before its sweep starts.
    let frame = appearing.frame(lines.clone(), 0., at(t0, 316));
    assert_eq!(revealed(frame).map(|(top, _)| top), Some(0.));
    assert!(frame.height < 40., "{}", frame.height);
    let frame = appearing.frame(lines.clone(), 0., at(t0, 420));
    assert_eq!(revealed(frame).map(|(top, _)| top), Some(20.));
    // The second line is the last: 300 ms.
    let frame = appearing.frame(lines, 0., at(t0, 570));
    let (_, width) = revealed(frame).expect("sweeping");
    assert!((width - 100.).abs() < 1., "{width}");
}

#[test]
fn the_height_opens_to_the_next_line_before_the_sweep_ends() {
    let t0 = Instant::now();
    let mut appearing = Appearing::new();
    let lines = vec![line(0, 430.), line(1, 430.), line(2, 100.)];
    let start = appearing.frame(lines.clone(), 0., t0);
    assert_eq!(start.height, 20.);
    // 300 ms sweep: the height waits until 100 ms are left.
    let early = appearing.frame(lines.clone(), 0., at(t0, 150));
    assert_eq!(early.height, 20.);
    appearing.frame(lines.clone(), 0., at(t0, 210));
    let opening = appearing.frame(lines.clone(), 0., at(t0, 260));
    assert!(
        opening.height > 20. && opening.height < 40.,
        "{}",
        opening.height
    );
    let opened = appearing.frame(lines, 0., at(t0, 320));
    assert_eq!(opened.height, 40.);
}

#[test]
fn the_final_message_finishes_faster() {
    let t0 = Instant::now();
    let mut appearing = Appearing::new();
    let lines = vec![line(0, 100.)];
    appearing.frame(lines.clone(), 0., t0);
    appearing.frame(lines.clone(), 0., at(t0, 400));
    appearing.text_changed(true);
    let longer = vec![line(0, 300.)];
    appearing.frame(longer.clone(), 0., at(t0, 416));
    // A first sweep again (160 ms), not 300.
    let frame = appearing.frame(longer, 0., at(t0, 416 + 160));
    assert_eq!(frame.cut, RevealCut::All);
}

#[test]
fn every_line_shows_once_the_reveal_is_over() {
    let t0 = Instant::now();
    let mut appearing = Appearing::new();
    let lines: Vec<RevealLine> = (0..4).map(|ix| line(ix, 300.)).collect();
    let mut now = t0;
    let mut frame = appearing.frame(lines.clone(), 0., now);
    for _ in 0..200 {
        if !frame.moving {
            break;
        }
        now += Duration::from_millis(16);
        frame = appearing.frame(lines.clone(), 0., now);
    }
    assert_eq!(frame.cut, RevealCut::All);
    assert_eq!(frame.height, 80.);
}

#[test]
fn the_bubble_widens_with_the_sweep_and_never_narrows() {
    let t0 = Instant::now();
    let mut appearing = Appearing::new();
    let lines = vec![line(0, 400.), line(1, 100.)];
    let first = appearing.frame(lines.clone(), 40., t0);
    assert_eq!(first.width, Some(40.), "the footer's room past the edge");
    let mid = appearing.frame(lines.clone(), 40., at(t0, 150));
    let width = mid.width.expect("narrower than the text");
    assert!(width > 40. && width < 400., "{width}");
    // On the second line the revealed edge starts over; the bubble stays.
    let mut now = at(t0, 150);
    let mut frame = mid;
    while revealed(frame).is_some_and(|(top, _)| top == 0.) {
        now += Duration::from_millis(16);
        frame = appearing.frame(lines.clone(), 40., now);
    }
    assert_eq!(frame.width, None, "as wide as the text from then on");
}

#[test]
fn a_line_with_lines_below_takes_its_width_share_of_300_ms() {
    let t0 = Instant::now();
    let mut appearing = Appearing::new();
    // 860 px is twice `msgMaxWidth`: 600 ms.
    let lines = vec![line(0, 860.), line(1, 100.)];
    appearing.frame(lines.clone(), 0., t0);
    let frame = appearing.frame(lines, 0., at(t0, 300));
    let (_, width) = revealed(frame).expect("sweeping");
    assert!((width - 430.).abs() < 1., "{width}");
}
