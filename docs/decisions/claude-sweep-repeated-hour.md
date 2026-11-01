# Picked times in a repeated or skipped hour

## What Telegram Desktop does

`ChooseDateTimeBox::collect` in `ui/boxes/choose_date_time.cpp` builds
`QDateTime(state->date.current(), time)` and checks it against
`now + kMinimalSchedule`. Qt resolves a local time that shows twice (the
hour repeated when clocks go back) to its first showing, the one still on
daylight time. Before Qt 6.7 that is the legacy rule; from 6.7 it is the
`RelativeToBefore` default. A time the clock skips moves forward by the
length of the gap.

So during the second pass of a repeated hour, tdesktop reads 01:15 as the
01:15 that already went by and rejects it as being in the past, although
the clock will show 01:15 again in ten minutes.

## What Quill does

`schedule::civil_to_unix_with` resolves a time that shows twice to the
first showing after now. When both showings are ahead, that is the first,
as in tdesktop. When only the second is ahead, Quill takes it. That is the
one place where Quill differs from tdesktop: the user picked a time the
clock will still show, so Quill accepts it.

When both showings are past (the composer date box and the away schedule
accept past times), Quill takes the first, as Qt does.

A skipped time lands as far past the gap as it was into it, as Qt does:
02:30 on the night clocks go forward becomes 03:30.

`schedule::picked_instant` keeps the instant a picker opened at when the
picked time is left as it was. A message set for 01:20 CST and reopened
the evening before stays on 01:20 CST, not 01:20 CDT. tdesktop has no
such rule: it rebuilds the time from the fields, so the same reopen moves
the message an hour earlier.

## Out of this slice

- A time picked in the minute before its first showing (01:30 picked at
  01:29:58 CDT) resolves to that first showing and gets "too soon".
