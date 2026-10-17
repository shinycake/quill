# Shared-media row dates

## What tdesktop does

`overview/overview_layout.cpp:1029,1376` (voice, file and link rows) call `langDateTime`, which is `lng_mediaview_date_time` ("{date} at {time}") over `langDayOfMonth` and `QLocale().toString(time, ShortFormat)` (`lang/lang_keys.cpp:190`).

- Year: `langDateMaybeWithYear` (`lang_keys.cpp:20-60`). The year shows only when the date's year differs from the current one AND either year is more than one apart, or the years are neighbours and the months are more than three months apart across the new year (`month + 12 > otherMonth + 3`). A December date seen in January or February has no year.
- Clock: the system locale's short time format (12 or 24 hour).

## What Quill did

`date_time` in `src/state/shared_media_meta.rs` showed the year whenever the year differed from the current one, and always printed 24-hour `HH:MM`.

## What changed

The year follows tdesktop's rule exactly (`needs_year`), unit tested for same year, neighbouring years on both sides of the 3-month edge, and more than a year apart.

## Not changed: 12/24-hour clock

Quill has no time-format setting and no system-locale clock reader; every time in the app (chat list, bubbles, 34 `hhmm` call sites) is 24-hour. Reading the system clock style needs macOS, Windows and Linux backends and should switch all those sites together. Changing only these rows would make the app inconsistent with itself, so it is left for a separate change.
