# Story viewer header: time, privacy badge, profile link

## What Telegram Desktop does

`media/stories/media_stories_header.cpp`:

- `:207-265` `ComposeTimestamp`/`ComposeDetails`: "just now" under a minute,
  "N minutes ago" under an hour, "N hours ago" under 12 hours, then "today at
  {time}", "yesterday at {time}", else a date with time. " • edited" follows
  when the story was edited. The label is rebuilt when its bucket changes
  (`Timestamp::changes`).
- `:248-257` `ComposeCounter` and `:487-509`: the position " • N/M" is its own
  label after the name. The name gets what is left of the row after the
  counter, so the name truncates and the counter never does.
- `:544-572` `setVideoStreamViewers`: while a live stream plays, the second
  line is the LIVE badge and "N viewers"; `updateDateText` returns early for
  a stream (`:964-967`), so there is no age and no "edited". The right side
  has the volume toggle.
- `:79-100`, `:437`: a badge on the photo's corner for Close Friends (green,
  `historyPeer2UserpicBg`), Contacts (purple, `historyPeer5UserpicBg`) or
  Selected contacts (orange, `historyPeer8UserpicBg`) stories, drawn as a
  top-to-bottom gradient to the matching `...UserpicBg2`; none for Everyone.
- `:738-765` `toggleTooltip`: the `lng_stories_about_*` text (the `_my`
  variants for your own stories). It shows while the pointer is over the
  badge (`:437-452` hooks `MouseMove`/`Leave`). The brief said "click"; tdesktop
  uses hover, so Quill does too.
- `:343-345`: pressing the header opens the peer's info; your own stories are
  named "My Story" (`lng_stories_my_name`, `:355`).

## What Quill did

The header was "Name . Story N of M". No time, no badge, no link, and a
hard-coded English "edited" under the caption.

## What changed

- `quill::story_header` (core): `age` picks the bucket from the local UTC
  offsets, `privacy_badge` reads the raw `privacy_settings`.
- The header shows the poster's photo with the badge, the name with
  " • N/M" after it when there are several stories, and the age with
  " • edited" under it. The identity takes the free width (`flex_1`); the
  counter, the live controls and Close keep their size, so only the name
  truncates.
- A live story being watched shows the LIVE badge and its viewer count on the
  second line instead of the age, and only the volume and PiP icons on the
  right (Close stays where Quill has it). All strings are tdesktop's `lng_*` keys. The age is computed from the
  clock on each draw; the viewer's 100 ms tick redraws it, so it ages without
  a timer of its own.
- The badge has a hover tooltip with the `lng_stories_about_*` text (the markup
  tdesktop bolds is dropped). Press on the photo or name closes the viewer and
  opens the poster's profile, as the existing "Stories" button does. A poster
  with no profile (a basic group) gets no pointer and no button role.
- "edited" moved from the caption area into the header, as in tdesktop.
- Older stories show `dd.mm.yy` with the time through `lng_mediaview_date_time`.
  tdesktop uses the locale's short date.
