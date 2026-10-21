# Time format round

## What tdesktop does

- Clock: every time is `QLocale().toString(time, ShortFormat)`, so the 12/24-hour style comes from the system (Windows user time format, macOS "24-hour time", Linux `LC_TIME` locale).
- Day separators: `ItemDateText` / `ServiceMessagePainter::PaintDate` print `langDayOfMonthFull`, which adds the year by `langDateMaybeWithYear` (lang_keys.cpp:20-58): only when more than a year away, or in a neighbouring year more than three months away across the new year.
- Edited row: `AddWhenEditedForwardedAuthorActionHelper` adds `WhenReadContextAction(Api::WhenEdited(...))`. The row paints the `whenEdited` pencil icon and `FormatReadDate` text ("today at 12:36", "yesterday at ...", "Oct 6 at ..."), with no "Edited" word. Past the year rule the date is the locale's short numeric date (`langDayOfMonthShort`).

## What changed

- `src/clock_style.rs`: `ClockStyle` read from the system on macOS (`CFDateFormatterCreateDateFormatFromTemplate("j")`), Windows (`GetLocaleInfoEx(LOCALE_STIMEFORMAT)`) and Linux (`nl_langinfo_l(T_FMT)` over a private `newlocale`, so the process locale is untouched). Cached 10 seconds. `QUILL_CLOCK_FORMAT=12|24` overrides it; tests pin a style per thread (`ClockStyle::pin`). Other unix systems fall back to 24-hour.
- `local_time::hhmm` is the one formatter: every bubble time, chat-list stamp, service message, media viewer date, call log, scheduled and menu date already went through it. 12-hour prints `9:42 PM`.
- `local_time::needs_year` is the single `langDateMaybeWithYear`; day separators, the media viewer stamp and shared-media rows (moved from #991) use it.
- The Edited menu row is the pencil icon plus the bare `FormatReadDate` text; the read, original and edited rows share `original_date_text`. The send-effect preview's time box widens for the 12-hour clock.

## Left as tdesktop has it

Business hours (`FormatDayTime`), statistics axes, copied-message prefixes and exports use fixed `HH:MM` in tdesktop too. AM/PM markers are English; Quill has no locale-specific time patterns elsewhere either. The short numeric date is `dd.mm.yy`, as the chat list already used.

## How verified

Unit tests (formats, pattern detection, year rule, edited label, pinned style), a GPUI test for the Edited row, and light/dark captures of the `ready-origin-menus` demo with `QUILL_DEMO_ORIGIN=times` at 12 and 24 hours. The Windows and Linux readers are compile-checked only; this Mac's real setting was read as 24-hour.
