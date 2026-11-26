//! The rows follow the composer's height over 250 ms, and at once while a
//! bar moves, with the layout's glide off, or with animations off.

use super::ComposerGlide;
use std::time::{Duration, Instant};

const MS: u16 = 250;

fn at(start: Instant, ms: u64) -> Instant {
    start + Duration::from_millis(ms)
}

#[test]
fn a_new_line_glides_the_rows_to_the_new_height() {
    quill::power_saving::isolate_for_tests();
    let start = Instant::now();
    let mut glide = ComposerGlide::new(56., start);
    assert_eq!(glide.follow(80., MS, start), 56.);
    let mid = glide.follow(80., MS, at(start, 125));
    assert!(mid > 56. && mid < 80., "on its way: {mid}");
    assert!(glide.moving(at(start, 125)));
    assert_eq!(glide.follow(80., MS, at(start, 250)), 80.);
    assert!(!glide.moving(at(start, 250)));
}

#[test]
fn the_first_measure_is_where_the_rows_start() {
    quill::power_saving::isolate_for_tests();
    let start = Instant::now();
    let mut glide = ComposerGlide::new(0., start);
    assert_eq!(glide.follow(56., MS, start), 56.);
    assert!(!glide.moving(start));
}

#[test]
fn classic_follows_at_once() {
    quill::power_saving::isolate_for_tests();
    let start = Instant::now();
    let mut glide = ComposerGlide::new(56., start);
    assert_eq!(glide.follow(80., 0, start), 80.);
    assert!(!glide.moving(start));
}

#[test]
fn a_moving_bar_snaps_and_a_quiet_one_glides_again() {
    quill::power_saving::isolate_for_tests();
    let start = Instant::now();
    let mut glide = ComposerGlide::new(56., start);
    glide.bar_moved(start);
    assert_eq!(glide.follow(70., MS, at(start, 16)), 70.);
    // Mid-glide, a bar starts moving: the glide gives way.
    let later = at(start, 1000);
    glide.follow(100., MS, later);
    assert!(glide.moving(later));
    glide.bar_moved(at(later, 10));
    assert_eq!(glide.follow(110., MS, at(later, 20)), 110.);
    // Long after the bar stopped, a new line glides.
    let quiet = at(later, 500);
    assert_eq!(glide.follow(130., MS, quiet), 110.);
    assert!(glide.moving(quiet));
}

#[test]
fn with_animations_off_the_rows_follow_at_once() {
    quill::power_saving::isolate_for_tests();
    quill::power_saving::set_force_all(true);
    let start = Instant::now();
    let mut glide = ComposerGlide::new(56., start);
    assert_eq!(glide.follow(80., MS, start), 80.);
    quill::power_saving::set_force_all(false);
}
