# Telegram Business settings (part 1: hub, greeting, away, hours, location)

Parity item `parity:business-away-greeting`. This PR covers the hub, greeting
and away messages, opening hours and location. Start page, links to chat
and connected bots come in a follow-up, so the item is not ticked yet and
there is no parity fragment.

## What tdesktop does

- `settings/sections/settings_business.cpp`: the "Telegram Business" hub
  lists Greeting Messages, Away Messages, Quick Replies, Opening Hours,
  Location, Links to Chat, Custom Intro and Chatbots (fallback order). The
  subtitle says the features are unlocked for Premium users. Without
  Premium a row opens the Premium preview; Chatbots is the one row that
  works without Premium.
- `settings/business/settings_greeting.cpp`: a "Send Greeting Message"
  switch, the messages in the quick reply shortcut `hello` ("Create a
  Greeting Message" / "Edit messages" with the count), recipients, and
  "Period of no activity" (7, 14, 21 or 28 days; 7 by default).
- `settings/business/settings_away_message.cpp`: the same for the shortcut
  `away`, with a schedule (Send Always, Outside of Business Hours, Custom
  Schedule with start and end time, the next 24 hours by default) and "Only
  if Offline".
- Both switches refuse to turn on when the account is at its quick reply
  limit and the shortcut has no messages ("You have too many quick
  replies...").
- `settings/business/settings_recipients_helper.cpp`: "All 1-to-1 Chats
  Except..." or "Only Selected Chats", with the excluded or included chats
  and chat types (New Chats, Existing Chats, Contacts, Non-Contacts).
  Choosing "Only Selected" with nothing selected opens the picker and stays
  on "all except". Saving an exclusion drops the included list; saving an
  inclusion drops the excluded types; an empty inclusion falls back to
  "all except" (`MakeValid`). The picker lists private chats with users,
  not yourself, up to 100 (`edit_filter_chats_list.cpp`).
- `settings/business/settings_working_hours.cpp`: "Show Business Hours", a
  row per weekday with its hours and an open/closed switch (on means open
  24 hours), a day box with sets of opening and closing times (the last
  may close up to 06:00 the next day; "Add a Set of Hours" adds 08:00-20:00
  or four hours half an hour after the last set), and a time zone list
  from `help.getTimezonesList`, defaulting to the zone closest to the local
  offset.
- `settings/business/settings_location.cpp`: an address field and "Set
  Location on Map", which opens a map picker that needs map tokens from the
  app config. Without the tokens it shows only "You can set your location
  on the map from your mobile device."
- `data/business/data_business_info.cpp`: every screen saves when it is
  closed, sends nothing when the value is unchanged, and only sends a
  greeting or away message that has a shortcut.

## What changed

- `src/business_settings.rs`: recipients in tdesktop's shape
  (included/excluded/all-but-excluded) with `businessRecipients`
  conversion both ways, greeting and away settings parsing, the schedule,
  the request bodies (`setBusinessGreetingMessageSettings`,
  `setBusinessAwayMessageSettings`, `setBusinessOpeningHours`,
  `setBusinessLocation`, `getTimeZones`), time zone parsing, sorting and
  closest-zone matching.
- `src/business_hours_edit.rs`: a port of tdesktop's day interval code
  (`ExtractDayIntervals`, `ReplaceDayIntervals`, `normalized`,
  `IsFullOpen`, the add rule) in minutes, plus the day editor's
  clock-to-interval conversion.
- `src/business_info.rs`: `BusinessInfo` also carries the own greeting and
  away settings from `userFullInfo.business_info`.
- Settings domain: `SettingsPurpose::GetTimeZones` and
  `SaveBusinessSetting { kind }`, `SettingsPayload::TimeZones`,
  `BusinessSettingsState` (time zones, saves in flight, last error). The
  saved values come back through `updateUserFullInfo`, which TDLib sends
  after each `setBusiness*` (checked in `BusinessManager.cpp`: each query
  calls `on_update_my_user_*`).
- `src/connect/business_settings.rs`: driver methods; an invalid
  inactivity period or custom range is refused before sending.
- `src/ui/business/`: the dialog (hub, greeting, away, hours, weekday, time
  zone, location, recipients picker), registered with `register_dialogs!`
  at priority 5438, just after Quick Replies (5437). Settings gets a
  "Telegram Business" row after "Telegram Premium" and loses the top-level
  "Quick replies" row; quick replies are now opened from the hub, as in
  tdesktop, and from the `/` menu's edit row as before.
- One line in `session_apply_ok.rs` routes the `ok` of a save.

## Same as tdesktop

- Hub order, wording and Premium gate. The Quick Replies row opens the
  existing Quick Replies dialog.
- "Create a Greeting Message" / "Edit messages" open the Quick Replies
  dialog on the `hello` or `away` shortcut. It shows above the Business
  dialog, and closing it returns to the screen you were on with the
  unsaved choices kept.
- The quick reply limit check, the recipients rules, the picker limit, the
  inactivity periods, the default custom interval and the time zone
  default.
- Nothing is sent when the value did not change.

## Differences and gaps

- Each screen has Cancel and Save instead of saving on close. In a modal
  dialog, closing without a Save button would make Esc save.
- tdesktop's time pickers are drum pickers that offer "Next day" hours for
  the last set. Here each set has two time fields; a closing time at or
  before the opening time is read as the next day and labelled "(Next
  day)". Overlapping sets or a set closing after 06:00 next day get an
  error instead of being clamped by the picker.
- No map picker: the address can be edited, a point set on another device
  can be kept (with an "Open on map" link) or removed, and the fallback
  note is shown when there is no point. tdesktop without map tokens shows
  only the note.
- A refused save shows the error class in words ("invalid request"),
  because Quill never keeps TDLib's error text. tdesktop words
  `BUSINESS_RECIPIENTS_EMPTY` specially.
- The custom schedule uses the kit date picker without a lower bound
  (tdesktop's minimum is August 2013; earlier values are rejected on save).
- Without Premium, a hub row opens the Premium dialog; in demo mode that
  dialog does not open (an existing limit of `open_premium`).
- Not in this PR: Custom Intro (start page), Links to Chat and Chatbots,
  and the "Do Not Hide Ads" section.

## Verified

- Unit tests: `business_settings` (parsing, recipients JSON both ways,
  picker save rules, the chat limit, request JSON for greeting, every away
  schedule, opening hours, location, validation, time zones),
  `business_hours_edit` (day extraction and replacement, night shifts,
  Sunday into Monday, full days, summaries, added sets, clock rows), and
  `connect::tests::business_settings` (time zones once, save tracked until
  `ok`, refused save, invalid settings not sent, greeting and away read
  from `updateUserFullInfo`).
- Eight GPUI UI tests in `src/ui/business/tests.rs` (`--features
  demo-capture`): every screen renders, the Premium gate, quick replies
  above the dialog and back, the quick reply limit, picking chats, adding
  a set of hours, demo saves refused, away shortcut and range checks.
- Visual check of every screen with a temporary demo (captures kept out of
  the repository).
- gate.sh, `scripts/check-hotspots.sh origin/main`,
  `scripts/check-file-size.sh`.
- Not run against a live account: saving would change real business
  settings.
