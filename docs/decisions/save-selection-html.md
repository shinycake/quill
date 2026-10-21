# Save selected rich messages as HTML

Branch: `save-selection-html`. README id `parity:menu-save-selection-html`.
Reference: tdesktop `Iv::RichMessageHtmlExport`
(`iv/iv_rich_message_html_export.cpp`) and `AddSaveRichMessageHtmlAction`,
read only. Quill copies no tdesktop code.

## When the action shows up

tdesktop shows "Save as HTML" only when:

- exactly one item is selected,
- that item has a rich page,
- the chat has no copy restriction,
- and the item does not forbid forwarding.

It puts the entry in the context menu over a selection, next to "Download
files", and clears the selection once the action runs.

`quill::rich_html::selection_target` checks the same things against the
session:

- exactly one id is selected,
- the chat is not protected,
- the message has been sent (not pending or failed),
- its content is `RichMessage`,
- it is not self-destructing,
- and, once the message's menu has asked TDLib (`messageProperties`), its
  content may be saved.

tdesktop's `forbidsForward()` is per message: a bot can send with
`protect_content` into a chat that is not protected. TDLib reports this as
`message.can_be_saved`, and for ephemeral content as
`ephemeral_content.can_be_saved`. Quill's history rows don't keep that flag,
so the bar can't always know it. The worker therefore checks it again on the
`getMessage` answer and refuses the export when the flag is false or missing,
or when the message self-destructs. This is the same rule the chat export
uses (`export::model::parse_message`).

Quill has no context menu over a selection. Its selection actions (Forward,
Delete, Copy as Text, Save, Report) all sit in the selection bar, so "Save as
HTML" goes in the bar too, and only when `selection_target` returns a message.
Clicking it ends the selection once the export starts, as tdesktop does. If
the folder picker is dismissed, or fails to open (no desktop portal on Linux,
for example), the selection stays so the user can try again. A failure also
shows the "Could not save" toast.

## Where the file goes

The rules follow tdesktop:

- With "Ask where to save each file" off, Quill writes
  `<downloads>/<title>/<title>.html` and puts the media in `media/` next to the
  page.
- With it on, a folder picker opens first. It uses GPUI's
  `prompt_for_paths`, so it runs on macOS, Windows and Linux.
- With no download folder at all, tdesktop falls back to the session's own
  temp folder (`File::DefaultDownloadPath`). Quill uses the account's own
  `exports` folder for this, never the system temp folder. The toast then
  names that folder rather than calling it "Downloads".

The title is the first heading. If there is none, it is the first paragraph,
cut to 48 characters. If that is empty too, the chat's title is used. A clash
with an existing folder gets a numbered name (`create_named_folder`, shared
with the chat export).

## Rendering from TDLib's JSON

Quill's `RichBlock` model drops what the bubble does not draw: anchors, table
cell spans and alignment, list values, credits, and media sizes. The page
needs all of these, so the worker renders from TDLib's raw `richMessage`
instead. It calls `getMessage` first. If the page is not full, it also calls
`getFullRichMessage`, and it falls back to the partial page when that call
fails.

The markup uses tdesktop's tags and classes, for example `tg-spoiler`,
`tg-emoji`, `tg-math`, `tg-time`, `tg-collage` and `tg-slideshow`. The CSS and
the slideshow script are Quill's own and follow light and dark mode. The
script is included only when the page has a slideshow.

## Media, two passes

The page is rendered twice through a `MediaSource`:

1. A collector pass lists every photo, video, audio file and inline image,
   with its `media/` name. A case-insensitive clash adds `_<id>` to the name.
2. The worker fetches each file through TDLib and copies it. `fetch_file`
   refuses any path outside TDLib's files folder.
3. A second pass renders with the map of files that were actually saved.

Because of this, every `src` in the page points at a file that exists. A file
that failed to download becomes `div.media-missing`, as in tdesktop. Paths are
percent-encoded.

- **File extensions.** An extension taken from a file name must be letters and
  digits only. Windows drops a trailing dot or space when it creates a file,
  which would break the `src`.
- **Collages.** A collage is laid out in boxes of up to 10 tiles. The album
  layout returns nothing past 12 items, and tdesktop's collage disappears in
  that case.
- **Fragment links.** Anchors and `#` links are normalized the same way as
  tdesktop's `NormalizeFragmentId` (percent-decoded, trimmed, lowercased), so
  they always match.

## Left out, and why

- **Documents and button rows.** tdesktop writes nothing for them either.
- **`data-tg-src`.** TDLib does not expose the photo and document ids that
  tdesktop puts there.
- **Formula images.** Quill has no LaTeX renderer, so math goes in as source
  text, the way tdesktop's own fallback writes it.
- **Progress and cancel (known gap).** tdesktop adds the export to its download
  manager as an external item, with progress, a cancel button, and the saved
  page in the list afterwards. Quill's downloads panel only lists TDLib's own
  file downloads (`addFileToDownloads`). It has no external items, so Quill
  says nothing until the toast, and a long video download can't be stopped
  except by logging out or quitting. Adding external items to that panel is a
  separate change.

## Threads and safety

- **Threads.** The export runs on a `spawn_guarded` worker thread. It uses the
  chat export's `WorkerLink`, which routes TDLib answers back through the
  reducer (`RequestPurpose::ExportRichHtml`). The driver pumps it with the
  same `pump_export` as the account and chat exports. The step is called from
  `poll_chat_export_progress`, the poll loop's single hook for exports. It
  forwards the worker's requests, picks up the result and shows a toast:
  - Saved: "Message was saved to your {downloads} folder". Clicking it shows
    the page in the file manager (`platform::reveal_in_file_manager`).
  - Failed: "Could not save the message as HTML."
- **Polling.** The poll step reports a change only when an export has ended
  and its toast needs drawing. A running export doesn't keep the loop busy:
  the worker wakes it (`wake::UPDATES`) whenever it has a request or has
  finished.
- **Cleanup.** Until the page is written, a guard owns the folder. A failure,
  a cancel or a panic all remove it.
- **Panics.** If the worker panics (`spawn_guarded` catches it), a guard on
  its outcome records `Failed`. The export then never looks like it is still
  running, and the same message can be saved again.
- **Logout and quit.** Logging out or closing drops every running export
  together with the account and chat exports. Dropping the link cancels the
  worker, which removes its folder, and nothing is reported.
- **Nesting.** Rich text and blocks stop at depth 48, so a hostile page cannot
  overflow the stack (panic-safety rule 13).
- **Escaping.** All text and attributes are escaped. Links are kept only for
  http, https, tg, mailto, tel, ftp and `#` fragments.

## Verification

- Core tests in `src/rich_html/tests.rs` cover:
  - escaping and injection, and unsafe links,
  - entity tags, text blocks, lists and tables,
  - media `src` values and the missing-media placeholder,
  - collage and slideshow,
  - file names, the title and deep nesting,
  - the worker copying media with a fake TDLib,
  - refusing files outside TDLib's folder,
  - removing the folder after a failure and after a panic,
  - refusing content that can't be saved: `can_be_saved` false or missing,
    ephemeral content, self-destructing messages,
  - `selection_target`: pending, failed and self-destructing messages, and a
    menu that has learned `can_be_saved: false`,
  - fragment links, Windows-safe extensions, collages of 14 tiles, and a very
    tall album photo.
- `src/rich_html/handle.rs` tests that a panicking worker still ends as
  `Failed`.
- Driver tests in `src/connect/tests/chat_state/rich_html.rs` cover:
  - the request flow (nothing counts as finished while the export runs, and a
    finished export doesn't block saving the same message again),
  - the refusal in a protected chat,
  - logout dropping running exports.
- UI tests in `src/ui/selection_html_demo.rs` check:
  - the bar shows the button for one rich message only,
  - it never shows in a protected chat,
  - clicking it ends the selection,
  - each ending gets its own toast, and the poll asks for a redraw only when
    something ended.
- Captures: the `ready-save-html` demo in light and dark, and a sample page
  from `page_html` opened in a browser in both schemes.

Not checked:

- real TDLib data and media downloads,
- the toast's reveal on real hardware,
- the folder pickers on Windows and Linux.

The Windows package job runs only after merge (`package-windows.yml` is
`workflow_call` only).
