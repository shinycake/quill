# Shared media rows: file, link, voice and music details

## What Telegram Desktop does

`Overview::Layout` (`overview/overview_layout.cpp`) draws each list kind differently:

- Files (`Document::paint`, `:1445`): a thumbnail tile with the extension, the file name,
  the size (`StatusText::update`, `:362`) and the message date (`_date`, `:1478`).
- Music (`Document` with `songLayout()`): "Performer – Title" (`FormatSongName`,
  `ui/text/format_song_name.cpp:12`, the file name or "Unknown Track" when there is no tag) and
  "duration, size" (`:369`). No date line.
- Voice (`Voice`, `:1010`): the sender's name and "date, duration" (`lng_date_and_duration`).
- Links (`Link`, `:2150`): the page title (else the domain, capitalised), up to three lines of the
  description, then every link of the message in the accent colour. A click on the row opens the
  link (`Link::getState`).

## What Quill did

Every row was a glyph and one label, and the fallback labels ("Photo", "Unknown artist", ...) were
English literals in `SharedMediaItem::label_for`.

## What changed

- `SharedMediaItem` carries a `SharedMediaMeta` (`src/state/shared_media_meta.rs`) captured when the
  page is parsed: file name and size, track tags and duration, voice sender and duration, link
  title, description and URLs. `SharedMediaMeta::lines` turns it into the lines a row prints, so
  the wording is a pure function of state and unit-tested.
- `AudioContent` gains `size` (needed for "duration, size").
- `src/ui/shared_media_row.rs` draws a tile (extension, letter or glyph) beside those lines. The
  voice sender is resolved at render time from the session, so a name that arrives later shows up.
- A click on a link row opens the first URL; Go to message stays in the context menu.
- Fallback labels, "You" and the status patterns come from lang keys (`lng_in_dlg_*`,
  `lng_duration_and_size`, `lng_date_and_duration`, `qll_ops_gif`, `qll_ops_unknown_track`).
  Music labels now follow `FormatSongName` (the title alone without a performer).

## Deviations

- A link row has a letter tile; tdesktop shows the page's photo. Link preview photos need their
  own download path and are left out.
- A row lists at most five links.
- The date is `full_stamp` ("12 March 2026, 21:42"), Quill's existing long date.

## Verified

Unit tests in `shared_media_meta.rs` and `state/tests/shared_media.rs`; captures of the Files,
Music, Voice and Links tabs in light and dark (`ready-shared-media` demo,
`QUILL_DEMO_SHARED_MEDIA_TAB`).
