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
first showing the picker can take: at or after now plus the picker's
minimum lead (10 seconds for a scheduled message or a link expiry, 12
for a voice chat, 30 for a restriction, a minute for a poll deadline, the
channel's delay plus a minute for a suggested post, none for the composer
date box and the away schedule). When both showings are ahead, that is the first, as in
tdesktop. When only the second can be taken, Quill takes it. That is the
one place where Quill differs from tdesktop: the user picked a time the
clock will still show, so Quill accepts it. This covers the second pass of
the hour and also a time picked in the minute before its first showing:
01:30 picked at 01:29:58 CDT is 01:30 CST, not "too soon".

When both showings are before that point, Quill takes the first, as Qt
does, and the picker reports it as too soon or past where that matters.

A skipped time lands as far past the gap as it was into it, as Qt does:
02:30 on the night clocks go forward becomes 03:30.

`schedule::picked_instant` keeps the instant a picker opened showing when
the picked time is left as it was. A message set for 01:20 CST and reopened
the evening before stays on 01:20 CST, not 01:20 CDT. tdesktop has no
such rule: it rebuilds the time from the fields, so the same reopen moves
the message an hour earlier.
