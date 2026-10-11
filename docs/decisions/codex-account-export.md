# Export Telegram data

Slice: `parity:data-export-full`, `parity:platform-data-export`. Replaces the
earlier raw JSONL account dump (`src/account_export.rs`), which wrote
`Quill-export-<nonce>/` and a `manifest.json` that said the export was
incomplete. Also folds the remaining chat-export polish into the same HTML
pages (thumbnails, profile-picture placeholders, reply jumps).

## What tdesktop does

Read from `Telegram/SourceFiles/export/`:

- **Settings** (`export_settings.h`, `view/export_view_settings.cpp`). The full
  export box offers account information (personal info and profile photos
  together), contacts, the story archive, music on profiles, six chat types
  (personal, bots, private groups, private channels, public groups, public
  channels) with "Only my messages" on groups and channels, seven media kinds
  and one size limit (slider, 100 steps from 1 MB to 4000 MB, default 8 MB,
  photos on), active sessions, miscellaneous data, then a folder and the
  format (HTML, JSON, both). Personal and bot chats are always exported in
  full; public groups and channels can only export the viewer's own messages.
  Defaults: account information, contacts, stories, music, personal chats and
  private groups. The from/till date range is only on a single chat's export
  (`singlePeerFrom` / `singlePeerTill`), not on this box.
- **Steps** (`export_controller.cpp`). Chat list, personal information,
  profile pictures, stories, profile music, contacts (including frequent
  contacts), sessions, miscellaneous data, then each chat. Cancelling stops
  at the next step and keeps what was already written.
- **Layout** (`output/export_output_html.cpp`, `export_output_json.cpp`).
  `export_results.html` (personal information and one link per section),
  `lists/` (profile pictures, stories, profile music, contacts, frequent
  contacts, sessions, web sessions, chats, and left chats), `chats/chat_NN/`
  with that chat's `messages.html` pages and media folders, `profile_pictures/`,
  `stories/`, `profile_music/files/`, `css/`, `js/`, and one `result.json`.
  The folder is `DataExport_<date>`. Media names and the 1000-messages-per-file
  split are the single-chat export's. Reply links, the back button and a
  missing-message note are `CheckLocation`, `GoToMessage` and `GoBack` in
  `js/script.js`. A video or file that is saved gets `<path>_thumb.jpg` beside
  it. People without a saved photo get a colored circle with initials.
- **Left out by the filters.** A chat file follows the media kinds and the
  size limit. Profile pictures and stories do not follow the kinds; only the
  size limit applies. Protected content is not copied.

## What changed

Settings › Data & Storage opens "Export Telegram data" (`src/ui/account_export_box.rs`)
instead of starting the old JSONL dump. The box has tdesktop's sections, media
kinds, size slider, "Only my messages", folder picker and format. Miscellaneous
data is not a checkbox: it is a takeout-only file TDLib cannot fetch, so it is
omitted, not stubbed. While an export runs, the same dialog shows progress
(`src/ui/account_export_progress.rs`): a bar, the current step, Hide, Cancel
export, and Show my data when it ends. Hiding the box keeps the export running;
the result appears as a status note.

The worker (`src/export/account/`) runs on a thread. It sends raw TDLib
requests through the existing export channel; `ConnectDriver::pump_account_export`
forwards them with the `quill_account_export` extra marker, and
`apply_common_payload` hands the answer back. The UI thread only reads a small
`AccountProgress`. The folder is `DataExport_<date>` (a second export the same
day is `DataExport_<date> (2)`), mode `0700` on Unix.

| File | Role |
| --- | --- |
| `account/settings.rs` | sections, defaults, only-my rules, size and kind skips |
| `account/dialogs.rs` | main list and archive, classified and filtered, numbered `chats/chat_NN/` |
| `account/sections.rs` | personal info, profile pictures, stories, profile music, contacts, sessions |
| `account/html.rs` | `export_results.html` and the `lists/` pages |
| `account/json.rs` | one streamed `result.json` for every section and chat |
| `account/worker.rs` | the blocking routine, one chat at a time |
| `account/handle.rs` | the worker thread and the progress the session polls |

Sections that have rows become list pages and index links, in tdesktop's order
(chats, contacts, frequent contacts, profile pictures, stories, music,
sessions, web sessions). Chats reuse the single-chat exporter
(`export_chat`): two passes, spool on disk, 1000 messages per HTML file, media
folders under `chats/chat_NN/`. An empty chat is a row on the chats list and
does not get a folder. One chat Telegram refuses to page does not end the
export; its row says there are no messages. Protected and secret chats are
left out. Saved phone contacts come from Quill's `getQuillSavedContacts`
(Telegram's `contacts.getSaved`, which needs a takeout session). When that is
refused, the Telegram contacts stand in, without the dates they were added,
and the contacts page says so.

Chat-export polish, in the same pages a single chat and an account export
both write:

- Videos, GIFs, files and voice messages that are saved get a still thumbnail
  beside the file (`<path>_thumb.jpg`), used as a video poster or a file
  thumb. A file left out by kind or size does not fetch its thumbnail.
- Each message run shows a profile-picture placeholder: a colored circle with
  initials, the color from the peer id (tdesktop's export does not show the
  real photo next to messages either). List rows and the index use the same
  circles; the index uses the newest saved profile photo when there is one.
- `js/script.js` is Quill's own rewrite of tdesktop's three behaviors
  (`CheckLocation`, `GoToMessage`, `GoBack`). A reply on the same page jumps
  with `GoToMessage`; a reply in another file is still a plain link. The
  stylesheet is still Quill's own.

Memory stays bounded by the design, not by a measured chat size. Account
sections are small and written whole. Chats run one at a time through the
single-chat exporter, which spools each history page to `.spool/` and holds
one HTML page of messages plus the list of message ids for reply links. The
spool is removed when the export ends, including on cancel. No 5k-versus-50k
measurement was recorded.

Removed: `src/account_export.rs` and its `pub mod` line. The session field
and the driver pump now point at `crate::export::AccountExport`.

## Differences from tdesktop

- No miscellaneous data (`OtherData` / `lists/other_data.json`). Not a stub:
  the box does not offer it, because it is a takeout file TDLib cannot fetch.
- No left channels (`channels.getLeftChannels`). The chats list is the main
  list and the archive only.
- Forum topics are not walked separately and are not given their own folders.
  A chat is one `getChatHistory` (or a sender search when only the viewer's
  messages are kept).
- Frequent contacts keep TDLib's order but not the rating numbers tdesktop
  prints.
- Own stylesheet and script. None of tdesktop's CSS, images or JavaScript.
- Cancelling keeps what was already written and does not cancel a
  `downloadFile` that TDLib is already running.
- The single-chat date range stays on the chat export box, where tdesktop
  puts it.

## How it was checked

- Core unit tests in `src/export/` and `src/export/account/`, against a fake
  TDLib with fixtures. They cover the account folder layout and index order,
  section filtering, JSON-only output, media kinds and the size limit on chats
  and profile music, the contacts fallback, a chat Telegram refuses, cancel,
  progress labels, private non-overwriting folders, thumbnails, profile-picture
  placeholders, and same-page reply jumps. No real account was exported.
- Driver test: with only sessions chosen, the worker's requests go out tagged
  `quill_account_export`, one at a time, and the fixture answers reach it.
- UI tests (`--features demo-capture`): the box opens from Data & Storage,
  edits, refuses to start with nothing chosen, and a running export shows
  progress instead of the options.
- Not checked: the box on a real screen, and an export from a live account.
