//! Starting a video chat or live stream from a chat (tdesktop
//! `Calls::Instance::startOrJoinGroupCall` / `confirmLeaveCurrent`,
//! calls/calls_instance.cpp, and `ChooseJoinAsProcess` /
//! `ScheduleGroupCallBox`, calls/group/calls_choose_join_as.cpp).
//!
//! Before anything is created the viewer confirms leaving a call they are
//! in; then "Start" asks "Start a video chat in this group?" with a link to
//! schedule instead, and "Schedule" opens a date and time box that opens on
//! the next whole hour at least half an hour away, allows now + 12 s up to
//! the end of the seventh day ahead, and says when members get notified.
#![deny(clippy::indexing_slicing)]

use crate::ids::ChatId;
use crate::local_time::civil_at;
use crate::schedule::ScheduleError;

/// What "Start", "Schedule" or "Start Streaming" is about to create.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateKind {
    /// "Start video chat" / "Start live stream", or the call button.
    Now,
    /// "Schedule video chat" / "Schedule live stream".
    Schedule,
    /// "Start Streaming" in "Stream with other apps".
    Rtmp,
}

/// The call the viewer is in when they start another (tdesktop `inCall()`
/// / `inGroupCall()`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurrentCall {
    None,
    /// A one-to-one call that is not ending already.
    OneToOne,
    /// A joined video chat or live stream, or a scheduled one.
    Group {
        chat_id: Option<ChatId>,
        scheduled: bool,
        /// Its chat is a channel (a live stream).
        channel: bool,
    },
}

/// `lng_call_leave_to_other_sure*` / `lng_group_call_leave*_to_other_sure*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaveText {
    /// End the one-to-one call (`lng_call_leave_to_other_sure`,
    /// `_channel` when the new one is a live stream).
    EndCall { to_channel: bool },
    /// Leave the video chat or live stream you are in.
    LeaveGroupCall {
        from_channel: bool,
        to_channel: bool,
    },
}

/// What happens before the create flow goes on (`confirmLeaveCurrent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeforeCreate {
    /// Nothing in the way.
    Proceed,
    /// You are in this chat's call already: bring its window up.
    ShowCurrent,
    /// Ask first; the call ends only when the new one is created.
    Ask(LeaveText),
}

/// tdesktop `confirmLeaveCurrent` with the default `IfNowInAnother`.
pub fn before_create(current: CurrentCall, target: ChatId, to_channel: bool) -> BeforeCreate {
    match current {
        CurrentCall::None => BeforeCreate::Proceed,
        CurrentCall::OneToOne => BeforeCreate::Ask(LeaveText::EndCall { to_channel }),
        CurrentCall::Group { chat_id, .. } if chat_id == Some(target) => BeforeCreate::ShowCurrent,
        // A scheduled call is simply replaced.
        CurrentCall::Group {
            scheduled: true, ..
        } => BeforeCreate::Proceed,
        CurrentCall::Group { channel, .. } => BeforeCreate::Ask(LeaveText::LeaveGroupCall {
            from_channel: channel,
            to_channel,
        }),
    }
}

/// The box's earliest time: 12 seconds from now (`min`).
pub const SCHEDULE_MIN_AHEAD_SECS: i64 = 12;

/// Start of the local hour `now` is in. `offset` is the local UTC offset
/// at a moment (`local_time::utc_offset_at`; a fixed zone in tests).
fn hour_start(now: i64, offset: &impl Fn(i64) -> i32) -> i64 {
    let civil = civil_at(now, offset(now));
    now - now.rem_euclid(60) - i64::from(civil.minute) * 60
}

/// Where the box opens: at least half an hour later, at zero minutes
/// (`QTime(hour, 0)` plus one hour, or two past half past).
pub fn default_schedule_at(now: i64, offset: &impl Fn(i64) -> i32) -> i64 {
    let minute = civil_at(now, offset(now)).minute;
    hour_start(now, offset) + if minute < 30 { 3600 } else { 7200 }
}

/// The latest time: one second before local midnight eight days from
/// today (`QDateTime(currentDate().addDays(8), QTime(0, 0)) - 1`).
pub fn schedule_max(now: i64, offset: &impl Fn(i64) -> i32) -> i64 {
    let civil = civil_at(now, offset(now));
    let today = hour_start(now, offset) - i64::from(civil.hour) * 3600;
    // A daylight saving change on the way moves the eighth midnight by an
    // hour: snap to the nearest local midnight.
    let guess = today + 8 * 86_400;
    let at = civil_at(guess, offset(guess));
    let since_midnight = i64::from(at.hour) * 3600 + i64::from(at.minute) * 60;
    let midnight = if at.hour >= 12 {
        guess + 86_400 - since_midnight
    } else {
        guess - since_midnight
    };
    midnight - 1
}

/// `ChooseDateTimeBox::collect`: the picked time must lie within
/// `[now + 12 s, max]`.
pub fn check_schedule(date: i64, now: i64, max: i64) -> Result<i64, ScheduleError> {
    if date < now + SCHEDULE_MIN_AHEAD_SECS {
        Err(ScheduleError::TooSoon)
    } else if date > max {
        Err(ScheduleError::TooFar)
    } else {
        Ok(date)
    }
}

/// How long until the chat starts, for "… will start in {duration}."
/// (`lng_days` / `lng_hours` / `lng_minutes`, at least one minute).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Countdown {
    Days(i64),
    Hours(i64),
    Minutes(i64),
}

pub fn countdown(date: i64, now: i64) -> Countdown {
    let left = date - now;
    if left >= 86_400 {
        Countdown::Days(left / 86_400)
    } else if left >= 3600 {
        Countdown::Hours(left / 3600)
    } else {
        Countdown::Minutes((left / 60).max(1))
    }
}

/// The description refreshes this often (`kLabelRefreshInterval`).
pub const COUNTDOWN_REFRESH_SECS: u64 = 10;

#[cfg(test)]
mod tests {
    use super::*;

    const UTC: fn(i64) -> i32 = |_| 0;
    /// 2026-10-15 09:20:30 UTC.
    const MORNING: i64 = 1_792_056_030;

    #[test]
    fn a_call_you_are_in_is_confirmed_before_another_starts() {
        let target = ChatId(5);
        assert_eq!(
            before_create(CurrentCall::None, target, false),
            BeforeCreate::Proceed
        );
        assert_eq!(
            before_create(CurrentCall::OneToOne, target, true),
            BeforeCreate::Ask(LeaveText::EndCall { to_channel: true })
        );
        let group = |chat: i64, scheduled: bool| CurrentCall::Group {
            chat_id: Some(ChatId(chat)),
            scheduled,
            channel: true,
        };
        assert_eq!(
            before_create(group(5, false), target, false),
            BeforeCreate::ShowCurrent
        );
        assert_eq!(
            before_create(group(6, true), target, false),
            BeforeCreate::Proceed
        );
        assert_eq!(
            before_create(group(6, false), target, false),
            BeforeCreate::Ask(LeaveText::LeaveGroupCall {
                from_channel: true,
                to_channel: false
            })
        );
    }

    #[test]
    fn the_box_opens_on_a_whole_hour_at_least_half_an_hour_away() {
        // 09:20 → 10:00.
        assert_eq!(default_schedule_at(MORNING, &UTC), MORNING - 1230 + 3600);
        // 09:40 → 11:00.
        let late = MORNING + 20 * 60;
        assert_eq!(default_schedule_at(late, &UTC), MORNING - 1230 + 7200);
        // A zone half an hour off UTC: 14:50 local → 16:00 local.
        let india = |_: i64| 5 * 3600 + 1800;
        let at = default_schedule_at(MORNING, &india);
        let civil = civil_at(at, india(at));
        assert_eq!((civil.hour, civil.minute), (16, 0));
        assert!(at - MORNING >= 30 * 60);
    }

    #[test]
    fn the_range_ends_before_midnight_eight_days_ahead() {
        let max = schedule_max(MORNING, &UTC);
        let civil = civil_at(max, 0);
        assert_eq!((civil.hour, civil.minute), (23, 59));
        // 2026-10-15 + 7 days.
        assert_eq!(civil.day, 22);
        assert_eq!(
            check_schedule(MORNING + 5, MORNING, max),
            Err(ScheduleError::TooSoon)
        );
        assert_eq!(check_schedule(MORNING + 12, MORNING, max), Ok(MORNING + 12));
        assert_eq!(check_schedule(max, MORNING, max), Ok(max));
        assert_eq!(
            check_schedule(max + 1, MORNING, max),
            Err(ScheduleError::TooFar)
        );
    }

    #[test]
    fn the_countdown_uses_the_largest_whole_unit() {
        assert_eq!(
            countdown(MORNING + 2 * 86_400 + 5, MORNING),
            Countdown::Days(2)
        );
        assert_eq!(
            countdown(MORNING + 3 * 3600 + 59, MORNING),
            Countdown::Hours(3)
        );
        assert_eq!(countdown(MORNING + 125, MORNING), Countdown::Minutes(2));
        assert_eq!(countdown(MORNING + 20, MORNING), Countdown::Minutes(1));
    }
}
