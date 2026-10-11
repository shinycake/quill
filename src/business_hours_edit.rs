//! Editing business opening hours one weekday at a time, a port of
//! tdesktop's `data_business_common.cpp` (`ExtractDayIntervals`,
//! `ReplaceDayIntervals`, `IsFullOpen`, `WorkingIntervals::normalized`)
//! and the day box of `settings_working_hours.cpp`, in minutes instead of
//! seconds (TDLib's `businessOpeningHoursInterval` unit).
//!
//! Week intervals start on Monday 00:00 (minute 0). A day's intervals are
//! relative to that day's midnight; the last one may close up to 06:00 of
//! the next day ([`IN_NEXT_DAY_MAX`]).

/// `(start, end)` in minutes; empty when `start >= end`.
pub type Interval = (i32, i32);

pub const DAY: i32 = 24 * 60;
pub const WEEK: i32 = 7 * DAY;
/// tdesktop `kInNextDayMax`: a day may close at most 6 hours into the next.
pub const IN_NEXT_DAY_MAX: i32 = 6 * 60;

pub const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

fn valid(i: Interval) -> bool {
    i.0 < i.1
}

fn shifted(i: Interval, by: i32) -> Interval {
    (i.0 + by, i.1 + by)
}

fn intersected(a: Interval, b: Interval) -> Interval {
    let result = (a.0.max(b.0), a.1.min(b.1));
    if valid(result) { result } else { (0, 0) }
}

fn sort_and_merge(mut list: Vec<Interval>) -> Vec<Interval> {
    list.sort_by_key(|i| i.0);
    let mut out: Vec<Interval> = Vec::with_capacity(list.len());
    for interval in list.into_iter().filter(|i| valid(*i)) {
        match out.last_mut() {
            Some(last) if interval.0 <= last.1 => last.1 = last.1.max(interval.1),
            _ => out.push(interval),
        }
    }
    out
}

/// tdesktop `MoveTailToFront`: what runs past the end of the week wraps to
/// Monday.
fn move_tail_to_front(mut list: Vec<Interval>) -> Vec<Interval> {
    let after = (WEEK, WEEK + DAY);
    while let Some(&last) = list.last() {
        let tail = intersected(last, after);
        if !valid(tail) {
            break;
        }
        let n = list.len() - 1;
        list[n].1 = tail.0;
        if !valid(list[n]) {
            list.pop();
        }
        list.insert(0, shifted(tail, -WEEK));
    }
    list
}

/// tdesktop `WorkingIntervals::normalized`.
pub fn normalized(list: Vec<Interval>) -> Vec<Interval> {
    sort_and_merge(move_tail_to_front(sort_and_merge(list)))
}

/// tdesktop `ExtractDayIntervals`: the intervals of weekday `day` (Monday
/// 0) relative to its midnight.
pub fn extract_day(week: &[Interval], day: usize) -> Vec<Interval> {
    let day = day.min(6) as i32;
    let mut list = Vec::new();
    for &interval in week {
        let now = intersected(interval, ((day - 1) * DAY, (day + 2) * DAY));
        let after = intersected(interval, ((day + 6) * DAY, (day + 9) * DAY));
        let before = intersected(interval, ((day - 8) * DAY, (day - 5) * DAY));
        if valid(now) {
            list.push(shifted(now, -day * DAY));
        }
        if valid(after) {
            list.push(shifted(after, -(day + 7) * DAY));
        }
        if valid(before) {
            list.push(shifted(before, -(day - 7) * DAY));
        }
    }
    let mut list = normalized(list);
    list.retain(|i| !(i.1 <= 0 || i.0 >= DAY));
    if let Some(last) = list.last_mut() {
        if last.0 <= 0 && last.1 >= DAY {
            *last = (0, DAY);
        } else if last.1 > DAY + IN_NEXT_DAY_MAX {
            *last = intersected(*last, (0, DAY));
        }
    }
    if let Some(&first) = list.first()
        && first.0 <= 0
    {
        if first.0 < 0 && first.1 <= IN_NEXT_DAY_MAX && first.0 > -DAY {
            list.remove(0);
        } else {
            list[0] = intersected(first, (0, DAY));
            if !valid(list[0]) {
                list.remove(0);
            }
        }
    }
    list
}

/// tdesktop `RemoveDayIntervals`.
fn remove_day(week: &[Interval], day: i32) -> Vec<Interval> {
    let mut list = normalized(week.to_vec());
    let today = (day * DAY, (day + 1) * DAY);
    let before = (0, today.0);
    let after = (today.1, WEEK);
    let mut i = 0;
    while i < list.len() {
        let current = list[i];
        if current.1 <= today.0 || current.0 >= today.1 {
            i += 1;
            continue;
        }
        // The previous day's interval that closes early this morning.
        if current.1 <= today.0 + IN_NEXT_DAY_MAX
            && (current.0 < today.0
                || (day == 0 && current.0 == today.0 && list.last().is_some_and(|l| l.1 >= WEEK)))
        {
            i += 1;
            continue;
        }
        let first = intersected(current, before);
        let second = intersected(current, after);
        if valid(first) {
            list[i] = first;
        } else if valid(second) {
            list[i] = second;
        } else {
            list.remove(i);
            continue;
        }
        i += 1;
    }
    normalized(list)
}

/// tdesktop `ReplaceDayIntervals`: put `replacement` (relative to the
/// day's midnight) in place of weekday `day`'s intervals.
pub fn replace_day(week: &[Interval], day: usize, replacement: &[Interval]) -> Vec<Interval> {
    let day = day.min(6) as i32;
    let mut list = remove_day(week, day);
    list.extend(replacement.iter().map(|&i| shifted(i, day * DAY)));
    normalized(list)
}

/// tdesktop `IsFullOpen` on an extracted day: 00:00-23:59 or 00:00-24:00.
pub fn is_full_open(day: &[Interval]) -> bool {
    day.iter().any(|i| valid(*i))
        && matches!(day.first(), Some(&(0, end)) if end == DAY - 1 || end == DAY)
}

/// tdesktop `FormatDayTime`: "09:00", or "02:00 (Next day)" past midnight
/// (and for 24:00 when `end_as_next_day`).
pub fn format_day_time(minute: i32, end_as_next_day: bool) -> String {
    let wrap = |m: i32| format!("{:02}:{:02}", m / 60, m % 60);
    if minute > DAY || (end_as_next_day && minute == DAY) {
        format!("{} (Next day)", wrap(minute - DAY))
    } else {
        wrap(if minute == DAY { 0 } else { minute })
    }
}

/// The weekday row's summary: "Closed", "Open 24 hours" or the ranges.
pub fn day_summary(day: &[Interval]) -> String {
    if !day.iter().any(|i| valid(*i)) {
        "Closed".into()
    } else if is_full_open(day) {
        "Open 24 hours".into()
    } else {
        day.iter()
            .map(|&(s, e)| {
                format!(
                    "{} - {}",
                    format_day_time(s, false),
                    format_day_time(e, false)
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// The interval "Add a Set of Hours" appends (tdesktop): 08:00-20:00 on an
/// empty day, otherwise four hours starting half an hour after the last
/// one. `None` when the day is full.
pub fn next_added_interval(day: &[Interval]) -> Option<Interval> {
    match day.last() {
        None => Some((8 * 60, 20 * 60)),
        Some(&(_, last)) if last + 1 < DAY => {
            let from = (last + 30).min(DAY - 30).max(last + 1);
            Some((from, from + 4 * 60))
        }
        Some(_) => None,
    }
}

/// Why the day editor cannot save.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayProblem {
    /// A set of hours overlaps or is out of order with the previous one.
    Overlap,
    /// The last set closes more than 6 hours into the next day, or an
    /// earlier one runs past midnight.
    TooLate,
}

impl DayProblem {
    pub fn line(self) -> &'static str {
        match self {
            Self::Overlap => "Each set of hours must start after the previous one closes.",
            Self::TooLate => "Only the last set of hours can close after midnight, by 06:00.",
        }
    }
}

/// Turn the day editor's clock times into intervals. A closing time at or
/// before the opening time is on the next day (tdesktop's "Next day"
/// picker values); `00:00` closing is midnight.
pub fn day_from_clock(rows: &[(i32, i32)]) -> Result<Vec<Interval>, DayProblem> {
    let mut out = Vec::with_capacity(rows.len());
    let mut previous_end = -1;
    for (index, &(open, close)) in rows.iter().enumerate() {
        let open = open.rem_euclid(DAY);
        let close = close.rem_euclid(DAY);
        let end = if close <= open { close + DAY } else { close };
        if open <= previous_end {
            return Err(DayProblem::Overlap);
        }
        let last = index + 1 == rows.len();
        if end > DAY + IN_NEXT_DAY_MAX || (!last && end > DAY) {
            return Err(DayProblem::TooLate);
        }
        out.push((open, end));
        previous_end = end;
    }
    Ok(out)
}

#[cfg(test)]
#[path = "business_hours_edit_tests.rs"]
mod tests;
