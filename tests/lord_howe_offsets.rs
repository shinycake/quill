//! The OS zone lookup across Lord Howe's spring change, which falls at
//! 15:30 UTC. Its own test binary, since it sets `TZ` for the process.
#![cfg(unix)]

use quill::local_time::{CivilTime, utc_offset_at};
use quill::schedule::civil_to_unix_with;

/// 2026-10-03 15:30 UTC: Lord Howe's 02:00 +10:30 becomes 02:30 +11.
const SWITCH: i64 = 1_791_041_400;

#[test]
fn a_time_in_lord_howes_skipped_half_hour_lands_after_it() {
    if !std::path::Path::new("/usr/share/zoneinfo/Australia/Lord_Howe").exists() {
        eprintln!("no tzdata for Australia/Lord_Howe; skipped");
        return;
    }
    // SAFETY: this binary runs only this test, so no other thread reads
    // the environment.
    unsafe { std::env::set_var("TZ", "Australia/Lord_Howe") };
    unsafe extern "C" {
        fn tzset();
    }
    // SAFETY: `tzset` takes no arguments and only rereads `TZ`.
    unsafe { tzset() };

    assert_eq!(utc_offset_at(SWITCH + 600), 39_600);
    assert_eq!(utc_offset_at(SWITCH - 600), 37_800);

    // 02:10 on 2026-10-04 never shows; it means 02:40 +11.
    let ten_past_two = CivilTime {
        year: 2026,
        month: 10,
        day: 4,
        hour: 2,
        minute: 10,
        weekday: 6,
    };
    let picked = civil_to_unix_with(&ten_past_two, utc_offset_at, SWITCH - 86_400);
    assert_eq!(picked, SWITCH + 600);
}
