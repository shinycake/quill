# Profile business rows and Fragment note

## What tdesktop does
- `info_profile_actions.cpp` (`CreateWorkingHours`): a row showing Open or
  Closed, with "opens in N units" or today's hours beside it. Tapping it
  expands the weekly schedule. When the business is in another time zone, a
  "local time" / "my time" link switches the schedule. A Location row shows
  the address and opens a map.
- `info_profile_phone_menu.cpp`: the phone row context menu of a number with
  a `888` prefix (not your own) gets a note that the number is not tied to a
  SIM card and was acquired on Fragment, with a link.

## What changed
- `src/business_info.rs` (new, pure): parses `userFullInfo.business_info`
  (location, `opening_hours`, `local_opening_hours`, `next_open_in`,
  `next_close_in`), merges and wraps intervals, computes open/closed and
  minutes until opening, formats day rows ("open 24 hours", "closed",
  night shifts as "(next day)") and detects Fragment numbers.
- `UserProfileExtras.business` carries it into the session.
- `src/ui/profile_business.rs` (new): hours and location rows and the
  Fragment note; two small hooks in `profile_panels/mod.rs`.
- Open/closed uses TDLib's `local_opening_hours` against the local clock, so
  no time zone database is needed; the snapshot fields are the fallback.
- The location row opens OpenStreetMap, as message locations do.

## Verified
- Unit tests in `src/business_info.rs`.
- Screenshot of a seeded demo profile (temporary fixture, not committed)
  showing the Closed state, "opens in 1 day", the local time link and the
  Location row.
- Not verified: the expanded schedule and the Fragment menu note were not
  captured (context menus are not capturable in the demo harness).
