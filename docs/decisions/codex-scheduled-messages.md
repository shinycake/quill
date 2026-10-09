# Scheduled messages (gap batch 10)

## What Telegram Desktop does

- `history_view_schedule_box.cpp` / `ui/boxes/choose_date_time.cpp`: the
  schedule box has a date field with a calendar and a time field. The
  default time is now + 10 minutes; a time must be at least 10 seconds ahead
  and at most a year away, otherwise the time field shows an error and
  nothing is sent. The title is "Send this message on..." or, in Saved
  Messages (`SendMenu::Type::Reminder`), "Remind me on...". A top-right menu
  offers "Send when online" in chats with another person (not self, not a
  bot, last seen not hidden).
- The send menu says "Schedule Message" / "Set a reminder"; the scheduled
  section is titled "Scheduled Messages" / "Reminders".
- Scheduled messages have "Send Now" and "Reschedule" in their context menu
  (`lng_context_send_now_msg`, `lng_context_reschedule`), implemented with
  `sendScheduledMessages` / `editMessageSchedulingState`.
- A scheduled-messages button appears in the composer while the chat has
  scheduled messages.

## What changed in Quill

- `quill::schedule` (new, pure): the 10 s / 1 year window, the 10 minute
  default, local wall clock to unix conversion across DST, and the
  Schedule/Reminder wording. Unit tested.
- `editMessageSchedulingState` request, `RequestPurpose::EditMessageSchedulingState`,
  `ConnectDriver::edit_scheduled_message`. On `ok` the reducer drops the
  entry (send now) or rewrites its send time (reschedule); on error a status
  note is shown and the entry stays.
- `chat.has_scheduled_messages` and `updateChatHasScheduledMessages` feed
  `Session::chat_has_scheduled_messages`; the composer shows a calendar-clock
  button that opens the scheduled list only while the flag is set.
- The +1h/+8h/+24h popup is replaced by the kit `DatePicker` with minute
  precision (24 h), days outside [today, today + 1 year] disabled, the same
  three durations as presets, an inline error, "Send when online" for
  one-to-one chats with another person, Cancel and Schedule/Remind.
- The scheduled list rows now have Send now, Reschedule (opens the picker,
  returns to the list afterwards), Edit and Delete, and use Reminders wording
  in Saved Messages.

## Differences from tdesktop

- Confirming the picker sets the composer's one-shot send time (the chip next
  to the input), as Quill already did, instead of sending immediately.
- "Send now" does not ask for confirmation.
- "Send when online" does not check hidden last-seen (Quill has no such
  flag); the server rejects it and the error is surfaced.
- No premium repeat period (`repeat_period` stays 0), no per-day thumbnails
  in the calendar, no silent toggle in the box.
- Reschedule uses the popup above the composer, so it needs the composer to
  be visible in that chat.

## Verification

- `cargo test` (core + ui): `schedule::tests`, request shape,
  send-now / reschedule / error reducer tests, `updateChatHasScheduledMessages`,
  driver validation.
- Visual: demo kind `ready-scheduled` with `QUILL_DEMO_SCHEDULED=button|picker|list|reminder|reminder-list`
  (`--screenshot-demo ready-scheduled`, `QUILL_DEMO_CAPTURE`).
- Not verified: against live Telegram; the date picker's open calendar
  popover and the right-click "Set a reminder" menu entry (static captures
  only); Windows and Linux builds (no platform-specific code was added).
