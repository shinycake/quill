# Slice spec: `parity:platform-data-export` — Full account data export

Telegram Desktop's "Export Telegram data" (Settings → Advanced), rebuilt for
Quill. TDLib has no account-export constructor, so the bundle is assembled
client-side from existing primitives.

## What was built

- `src/data_export.rs` — `DataExportState` (options, chat queue, media
  queue, counters, manifest), `new_bundle_dir` (`<dest>/quill-data-export-<unix>/`
  with numeric suffix on collision), `finalize` writing `account.json`,
  `contacts.json`, `media_manifest.json`. Media jobs dedup by file id;
  document originals keep their names, everything else is `file-<id>`,
  sanitized.
- `src/connect/data_export.rs` — `start_data_export` /
  `cancel_data_export` / `pump_data_export`, pumped from the app's poll loop
  (never the UI thread). Four phases: (1) contacts gate — waits for the
  existing deduped `getContacts` fetch; (2) chats — every non-secret chat is
  paged through the *shared* per-chat `ChatExportState` machinery from
  `parity:platform-history-export` (same JSON format, one file per chat in
  `chats/`); (3) media — the export arm of `session_apply_messages` queues
  each page's media files; the pump copies locally-available files straight
  into `media/` and downloads the rest with 4 concurrent `downloadFile`s
  (`AUTO_MEDIA_DOWNLOAD_PRIORITY`), recording a manifest of
  (chat, message) → file; (4) finalize.
- `pump_chat_export` (`src/connect/messages.rs`) grew a bundle-aware
  done-branch: inside an account export the per-chat JSON lands in the
  bundle's `chats/` and the batch advances instead of surfacing a note.
  Standalone per-chat export behavior is unchanged.
- UI: kit `Dialog` ("Export Telegram data") opened from a new
  "📦 Export Telegram data" entry in the hamburger menu next to the other
  settings-adjacent entries. Scope toggles (chat history / contacts /
  media), a destination folder field (empty = Downloads), a `Progress` bar
  with phase text while running, Cancel, and a completion view with the
  output path + stats. The per-chat Export button in the chat header is
  hidden while an account export owns the shared machinery.

## Bundle layout

```
quill-data-export-<unix>/
  account.json        # user (id, name, username, phone), stats, notes
  contacts.json       # contact list (id, name, username, phone)
  media_manifest.json # (chat_id, message_id) → media/ file name
  chats/<title>-<unix>.json   # the history-export per-chat format
  media/<file>        # downloaded/copied attachments + profile photo
```

## Deliberate omissions / honest limits

- **Secret chats are skipped** (counted in `account.json` notes). TDLib's
  `getChatHistory` technically covers them, but Quill's E2E layer is partial
  (state machine, composer gating) and exporting decrypted secret history to
  disk deserves its own security review.
- **Stories export** — out of scope per the slice brief.
- **No HTML output.** Telegram Desktop writes HTML+JSON per chat; ponytail:
  the history-export slice already defined the per-chat JSON format, so the
  account export extends it account-wide instead of inventing a second format.
- **No native folder picker** — no file-dialog dependency in the tree; the
  destination is a text field defaulting to Downloads.
- Media is best-effort: failed downloads are counted (`media_failed`) and
  don't abort the export; a chat whose paging fails aborts the batch with
  the error surfaced.
- Cancelling drops the export state; in-flight `downloadFile`s finish into
  TDLib's cache harmlessly.

## Out of this slice

- HTML rendering of the exported JSON (a separate viewer/export-format slice).
- Secret-chat history export (needs the E2E security review above).
- Scheduled/automatic exports; cloud upload of the bundle.
