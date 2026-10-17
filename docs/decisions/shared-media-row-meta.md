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
- Links follow `Link::Link` (`:2031-2124`): the listed addresses are the message's URL, text-link and
  e-mail entities (the page address when there are none), shown as the message writes them (a text
  link shows its label; a bare `github.com/x` gets `https://`). The description is the message text
  before the trailing run of those entities, else the page description. The row and the tile open
  the page URL (else the first link); each address line opens its own link. Every click goes through
  `queue_link` and `link_policy::open_decision`, so a look-alike host asks first, as in a chat.
- Voice rows name the sender (`from()->name()`), or "Forwarded from X" for a forward.
- File tiles lowercase the extension, squeeze it in the middle to fit, and colour by type
  (`DocumentGenericPreview`: blue documents, green sheets, red PDF and slides, yellow archives and
  media, else by the extension's first letter).
- Dates are `langDateTime`: "Oct 10 at 07:00", with the year when it is not this year
  (`lng_month_day`, `lng_month{N}_small`, `lng_mediaview_date_time`). A voice message with no date
  shows only its length.
- Fallback labels and the status patterns come from lang keys (`lng_in_dlg_*`,
  `lng_duration_and_size`, `lng_date_and_duration`, `qll_ops_gif`, `qll_ops_unknown_track`).
  Music labels now follow `FormatSongName` (the title alone without a performer).

## Deviations

- A link row has a letter tile; tdesktop shows the page's photo. Link preview photos need their
  own download path and are left out. A document tile does not show the file's thumbnail either.
- A row lists at most five links.
- An e-mail address is listed but not clickable.
- The voice caption tdesktop draws under the name is not shown.

## Verified

Unit tests in `shared_media_meta.rs` (message text before links, text links, schemeless
addresses, tile colour and elision, dates, forwards) and `state/tests/shared_media.rs`; a window
test (`shared_media_row_tests.rs`) that a look-alike address opens the confirm box; captures of the Files,
Music, Voice and Links tabs in light and dark (`ready-shared-media` demo,
`QUILL_DEMO_SHARED_MEDIA_TAB`).
