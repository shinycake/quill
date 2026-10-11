# Shared media calendar and month headers

## What Telegram Desktop does

The shared media tabs (`info/media/info_media_list_widget`) group items under
month headers, and the section menu has "Show calendar" / "Jump to date".
The calendar highlights days that have media of the tab's type
(`getChatMessageCalendar` in TDLib) and picking one reloads the list from
that date.

## What changed

- `SharedMediaItem` now carries `date`, and `state/shared_media_months.rs`
  groups items into local-month sections (`month_sections`). The gallery
  list (`ui/shared_media_list.rs`) draws a month header above each section
  for every tab.
- A Calendar button in the gallery header (hidden on GIFs, which have no
  calendar filter) opens the existing "Jump to date" box with the tab's
  filter (`Session::open_shared_media_calendar`). The box is the same
  `HistoryCalendar`; a new `shared_tab` field routes a picked day to the
  gallery instead of the history.
- Picking a day (or the nearest earlier day with media, else the nearest
  later one) sends `searchChatMessages` from that day's first message with a
  negative offset so the day's newer messages come along. The answer is
  trimmed so the list starts at the picked day (`trim_to_day`). A "Showing
  from <month>" strip with "Show latest" returns to the newest media.
- No new request type: `getChatMessageCalendar`, `GetChatMessageCalendar`
  and `GetSharedMedia` already existed.

## Limits

The calendar's Voice filter is `VoiceAndVideoNote`, slightly wider than the
Voice tab's `VoiceNote` filter, so a marked day can hold only a video note.
No demo shows the gallery, so the UI was not captured.

## Verification

`gate.sh` (fmt, clippy, tests): unit tests cover month grouping, the jump
window offset, trimming, nearest-day choice, opening the calendar per tab,
and a reducer test that a date-jump page starts at the picked day.
`check-hotspots.sh` and `check-file-size.sh` pass.
