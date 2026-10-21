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
- and it is not self-destructing.

Quill has no context menu over a selection. Its selection actions (Forward,
Delete, Copy as Text, Save, Report) all sit in the selection bar, so "Save as
HTML" goes in the bar too, and only when `selection_target` returns a message.
Clicking it ends the selection, as tdesktop does.

## Where the file goes

The rules follow tdesktop:

- With "Ask where to save each file" off, Quill writes
  `<downloads>/<title>/<title>.html` and puts the media in `media/` next to the
  page.
- With it on, a folder picker opens first. It uses GPUI's
  `prompt_for_paths`, so it runs on macOS, Windows and Linux.

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

## Left out, and why

- **Documents and button rows.** tdesktop writes nothing for them either.
- **`data-tg-src`.** TDLib does not expose the photo and document ids that
  tdesktop puts there.
- **Formula images.** Quill has no LaTeX renderer, so math goes in as source
  text, the way tdesktop's own fallback writes it.

## Threads and safety

- **Threads.** The export runs on a `spawn_guarded` worker thread. It uses the
  chat export's `WorkerLink`, which routes TDLib answers back through the
  reducer (`RequestPurpose::ExportRichHtml`). The poll loop forwards the
  worker's requests and picks up the result, then shows a toast:
  - Saved: "Message was saved to your {downloads} folder". Clicking it shows
    the page in the file manager (`platform::reveal_in_file_manager`).
  - Failed: "Could not save the message as HTML."
- **Cleanup.** A failed or cancelled export removes its folder.
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
  - removing the folder after a failure,
  - `selection_target`.
- Driver tests in `src/connect/tests/chat_state/rich_html.rs` cover the request
  flow and the refusal in a protected chat.
- UI tests in `src/ui/selection_html_demo.rs` check:
  - the bar shows the button for one rich message only,
  - it never shows in a protected chat,
  - clicking it ends the selection.
- Captures: the `ready-save-html` demo in light and dark, and a sample page
  from `page_html` opened in a browser in both schemes.

Not checked: real TDLib data and media downloads, the toast's reveal on real
hardware, and the folder pickers on Windows and Linux.
