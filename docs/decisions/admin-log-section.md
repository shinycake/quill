# Recent actions rebuilt after tdesktop's admin log

## What tdesktop does

`history/admin_log/` shows the log as a section with a fixed top bar (the
title "All actions" / "Selected actions", search, filter) over the events.
Events run oldest first with the newest at the bottom; older ones load when
the list is scrolled to the top. Each event is a service line ("X edited
message:", "X deleted message:", "X pinned message:") followed by the affected
message; an edit also carries the "Original message". Days are grouped like
the chat history. The filter box (`history_admin_log_filter.cpp`,
`InnerWidget::showFilter`) has "All actions" over per-category boxes in three
sections, "Show Actions by All Admins" over one box per admin with a photo,
and Cancel / Save. When nothing matches, a semibold title over a description
says whether a search, a filter or just the lack of events is the reason.

## What changed in Quill

- `src/ui/event_log.rs`: fixed header (title, Filter button, search, the
  explainer) over a gpui `list` (`ListAlignment::Bottom`) that shows the
  events oldest first. Paging, the 5 s poll and the reducer are #1147's and
  unchanged; `quill::event_log_list::plan_sync` tells the list whether the
  new events joined at the top (splice before, reader keeps their place), at
  the bottom (poll) or replaced everything (filter, search: start at the
  newest). The "load earlier" request fires from the list's scroll handler
  (deferred, the handler runs while the list is borrowed) and once per list
  length when a short list cannot scroll. A spinner pill shows at the top
  while earlier events remain.
- `src/ui/event_log_rows.rs`, `event_log_text.rs`: a row is a plain value
  (`LogEntry`) built before drawing, so nothing reads state inside the list
  closure. Avatar, service line with the actor in semibold, time, a day pill
  (the history's `service_pill`) on the first row of a day, and the message in
  the kit `Bubble` (muted variant). An edit shows the original text as a quote
  above the new text, the way a reply does.
- `src/ui/event_log_filter_box.rs` + `quill::event_log_filter`: the filter as
  a second page of the same dialog (`GroupSettingsView::RecentActionsFilter`,
  the dialog is modal, one at a time) with kit `Checkbox` rows. Choices stay in
  a draft until Save; Cancel drops them. tdesktop's grouped flags map onto
  TDLib's: "New members" is joins plus invites, the info box is info plus
  settings changes; channels have no topics or pinned-messages box.
- The search box is a debounced (400 ms) field again, with no Search button;
  it is created when the page opens.
- The page is no longer wrapped in the dialog's scroll area; it has its own
  height so the header stays put.
- Parser: an edit keeps `old_text`; message text is kept to 1000 characters
  (was 80) for the bubble.
- Strings: tdesktop's wording in `lang/en/admin_log.strings`; the chip labels
  and the old plain-text empty states are gone.

## Deviations

- TDLib has no "nothing selected" filter (an empty set means every type), so
  Save is disabled when no category or no admin is ticked. tdesktop lets the
  box close with nothing ticked.
- The service line names the actor first and the action after it, using
  Quill's existing action phrases for events other than message edits,
  deletions and pins (tdesktop has a full sentence per event; those are
  translated per language in `Telegram/Resources/langs`).
- The log is a dialog page, not a column of the main window.
- The affected message is shown from its text only (non-text content shows no
  bubble): `ChatEventAction` keeps the text, not the whole message.

## Verified

`ready-event-log` demo, light and dark: list, scrolled to the top (spinner),
Filter box, empty state (`QUILL_DEMO_EVENT_LOG=empty`). Unit tests for the
filter draft, the list planner and the parser; a GPUI UI test for the rows
and the filter flow.
