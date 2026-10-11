use super::*;

#[test]
fn extract_and_replace_a_plain_day() {
    let week = vec![(9 * 60, 17 * 60), (DAY + 9 * 60, DAY + 17 * 60)];
    assert_eq!(extract_day(&week, 0), vec![(540, 1020)]);
    assert_eq!(extract_day(&week, 1), vec![(540, 1020)]);
    assert!(extract_day(&week, 2).is_empty());
    let week = replace_day(&week, 2, &[(600, 720), (780, 1080)]);
    assert_eq!(extract_day(&week, 2), vec![(600, 720), (780, 1080)]);
    let week = replace_day(&week, 0, &[]);
    assert!(extract_day(&week, 0).is_empty());
    assert_eq!(extract_day(&week, 1), vec![(540, 1020)]);
}

#[test]
fn a_night_shift_stays_on_its_own_day() {
    // Monday 22:00 to Tuesday 02:00.
    let week = replace_day(&[], 0, &[(22 * 60, DAY + 2 * 60)]);
    assert_eq!(week, vec![(1320, 1560)]);
    assert_eq!(extract_day(&week, 0), vec![(1320, DAY + 120)]);
    // Tuesday does not show Monday's early-morning tail.
    assert!(extract_day(&week, 1).is_empty());
    // Replacing Tuesday keeps Monday's night shift.
    let week = replace_day(&week, 1, &[(600, 900)]);
    assert_eq!(extract_day(&week, 0), vec![(1320, DAY + 120)]);
    assert_eq!(extract_day(&week, 1), vec![(600, 900)]);
}

#[test]
fn sunday_night_wraps_to_monday() {
    let week = replace_day(&[], 6, &[(20 * 60, DAY + 3 * 60)]);
    assert_eq!(week, vec![(0, 180), (6 * DAY + 1200, WEEK)]);
    assert_eq!(extract_day(&week, 6), vec![(1200, DAY + 180)]);
    assert!(extract_day(&week, 0).is_empty());
}

#[test]
fn full_days() {
    let week = replace_day(&[], 3, &[(0, DAY)]);
    let thursday = extract_day(&week, 3);
    assert!(is_full_open(&thursday));
    assert_eq!(day_summary(&thursday), "Open 24 hours");
    assert!(is_full_open(&[(0, DAY - 1)]));
    assert_eq!(day_summary(&[]), "Closed");
}

#[test]
fn summaries_and_times() {
    assert_eq!(
        day_summary(&[(540, 780), (840, 1080)]),
        "09:00 - 13:00, 14:00 - 18:00"
    );
    assert_eq!(
        day_summary(&[(1320, DAY + 120)]),
        "22:00 - 02:00 (Next day)"
    );
    assert_eq!(format_day_time(DAY, false), "00:00");
    assert_eq!(format_day_time(DAY, true), "00:00 (Next day)");
}

#[test]
fn added_intervals_follow_tdesktop() {
    assert_eq!(next_added_interval(&[]), Some((480, 1200)));
    assert_eq!(next_added_interval(&[(480, 1200)]), Some((1230, 1470)));
    assert_eq!(next_added_interval(&[(480, DAY - 1)]), None);
}

#[test]
fn clock_rows_become_intervals() {
    assert_eq!(
        day_from_clock(&[(540, 780), (840, 1080)]),
        Ok(vec![(540, 780), (840, 1080)])
    );
    // A closing time before the opening time is the next day.
    assert_eq!(day_from_clock(&[(1320, 120)]), Ok(vec![(1320, DAY + 120)]));
    // Midnight closing.
    assert_eq!(day_from_clock(&[(540, 0)]), Ok(vec![(540, DAY)]));
    assert_eq!(
        day_from_clock(&[(540, 780), (700, 900)]),
        Err(DayProblem::Overlap)
    );
    assert_eq!(day_from_clock(&[(1320, 7 * 60)]), Err(DayProblem::TooLate));
    // Only the last set may run past midnight.
    assert_eq!(
        day_from_clock(&[(1320, 60), (120, 300)]),
        Err(DayProblem::TooLate)
    );
}

#[test]
fn normalized_merges_and_wraps() {
    assert_eq!(
        normalized(vec![(100, 200), (150, 300), (WEEK - 60, WEEK + 60)]),
        vec![(0, 60), (100, 300), (WEEK - 60, WEEK)]
    );
}
