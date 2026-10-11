# Chat export as HTML with media, date range and senders

Slice: `parity:data-chat-export-html`. Replaces the earlier in-memory export
(`parity:platform-chat-export`), which wrote one file with media as labels.

## What tdesktop does

Read from `Telegram/SourceFiles/export/`:

- **Settings** (`export_settings.h`, `view/export_view_settings.cpp`). For one
  chat the box offers seven media checkboxes (photos, videos, voice messages,
  video messages, stickers, GIFs, files), one size limit (slider, 100 steps
  from 1 MB to 4000 MB, default 8 MB), the format (HTML, JSON, both), a folder,
  and a from/till date range (`singlePeerFrom` / `singlePeerTill`). Only photos
  are on by default.
- **Filters** (`export_api_wrap.cpp`, `export_data_types.cpp`). A file is kept
  out when its kind is off or its size is over the limit; the HTML says so
  ("not included" or "exceeds maximum size, change data exporting settings").
  Messages are kept when `from <= date < till` (`SkipMessageByDate`).
- **Layout** (`output/export_output_html.cpp`). `messages.html`, then
  `messages2.html`, ... with 1000 messages each (`kMessagesInFile`); media in
  `photos/`, `video_files/`, `voice_messages/`, `round_video_messages/`,
  `stickers/`, `animations/`, `files/`; names like
  `photo_1@14-11-2023_22-13-20.jpg`; messages are written oldest first, with a
  date heading per day, a sender name above runs of messages from the same
  person within 15 minutes, reply and forward blocks, and service lines.
  `result.json` is the JSON output.
- **Senders.** tdesktop has no per-sender filter for one chat. "Senders" in the
  parity item means the sender names in the output, which tdesktop prints on
  every message. The "Only my messages" choice exists in tdesktop only for the
  whole-account export. Quill keeps its own "Only my messages" switch (from the
  earlier export) as an addition, and prints sender names.

## What changed

New core module `src/export/`:

| File | Role |
| --- | --- |
| `settings.rs` | media kinds, tdesktop's size steps, formats, date range, skip rules |
| `model.rs` | message model parsed from raw TDLib JSON (text, entities, media, replies, forwards, service lines) |
| `text.rs` | entity-aware, escaped text to HTML; only `http(s)`, `tg` and `mailto` links get an `href` |
| `names.rs` | tdesktop-style file names, unique per folder, sanitized |
| `html.rs` | page rendering, 1000 per file, Quill's own `css/style.css`, no script |
| `json.rs` | streamed `result.json` |
| `worker.rs` | the blocking export routine over a `Tdlib` trait |
| `handle.rs` | the worker thread and the channels the driver pumps |

How it runs: `ChatExport::start` makes a `ChatExport_<title>_<date>` folder in
the chosen directory and starts a thread. The thread sends raw TDLib requests
through a channel; `ConnectDriver::pump_chat_export` (called from the existing
poll loop) forwards them with the existing `quill_account_export` extra
marker and a new `SettingsPurpose::ExportChat`, and `apply_common_payload`
hands the answer back. The UI thread only reads a small `Progress` value.

Two passes, because TDLib pages newest first and the files read oldest first.
Pass one pages `getChatHistory` (starting from `getChatMessageByDate(till - 1)`
when an end date is set, and stopping once a page passes the start date),
applies the range and sender choice, resolves names once per person and spools
each page to a small file in `.spool/`. Pass two reads the spool backwards,
downloads media with a synchronous `downloadFile` (only for kinds that are on
and files under the limit), copies it into the export folder, and writes HTML
pages and `result.json` as it goes. Memory stays bounded: only one HTML page
of messages and the list of message ids (for reply links across files) are
held. Cancel is checked at every TDLib call and every message; the spool is
removed on any ending.

Safety: protected chats are refused (as before), messages that cannot be saved
or self-destruct never reach the files, and a file is only copied if its path
is inside TDLib's files directory.

UI: `src/ui/export_box.rs` (settings, calendar for the two dates, folder
picker through the OS dialog) and `src/ui/export_progress.rs` (progress bar,
Hide, Cancel export, Show folder). Hiding the box keeps the export running and
the result appears as a status note. The chat menu hook is unchanged
(`start_chat_export(chat_id, cx)`).

Removed: `src/chat_export.rs`, the `ExportChatHistory` purpose, its apply and
error branches, and `MessagesState::chat_export`.

## Differences from tdesktop

The later account-export slice
(`docs/decisions/codex-account-export.md`) adds thumbnails, profile-picture
placeholders and `js/script.js`. As this slice landed:

- No thumbnails for videos and files, no profile pictures, no `export_results.html`
  index page (tdesktop skips the index for one chat's export as well).
- No JavaScript. Replies link with plain anchors (`messages2.html#message12`).
- Own stylesheet, written for this export; none of tdesktop's CSS or images.
- Stories, polls with results, and many rare service messages print a short line
  or "(This message type is not exported)".
- Cancelling keeps what was already written and does not cancel a `downloadFile`
  that TDLib is already running.
- Quill keeps "Only my messages".

## How it was checked

- 57 core unit tests in `src/export/`: escaping and entity rendering (UTF-16
  offsets, overlap, link schemes), every message kind in HTML and JSON, file
  naming and sanitizing, size steps, kind and size filters, date range
  inclusivity, 2300 messages into three files with a cross-file reply link,
  short TDLib pages, cancel, TDLib errors, files outside TDLib's folder.
  All against a fake TDLib with fixtures; no real chat was exported.
- Driver tests: the worker's request goes out tagged and the answer reaches it
  through `Session`; protected chats are refused.
- UI tests (`--features demo-capture`): the box opens, edits, shows the calendar
  and closes without panicking.
- Not checked: the box on a real screen and an export from a live account.
