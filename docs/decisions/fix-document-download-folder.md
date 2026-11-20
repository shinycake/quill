# Downloaded documents go to the download folder

Contributed by `@pgiacome` (branch `fix/document-download-folder`). It fixes
a gap in an item already checked in STATUS.md
(`parity:data-download-path`), so there is no parity fragment.

## The problem

On Windows, with Settings > Data and Storage > Download folder set to a
folder on another drive, a PDF downloaded by clicking its row in a chat
never reached that folder. It stayed in TDLib's files directory
(`<data>\accounts\primary\files\documents`), and Open and Show in folder
pointed there.

TDLib writes downloads only into its own files directory. Quill read the
download folder setting in two places, "Save as…" and the media viewer's
Save, and never copied a finished chat download out of the cache. The
setting looked like it applied to every download, but it applied only to
those two actions. The code that does this is the same on every platform,
so Linux and macOS behaved the same way.

## Telegram Desktop

Telegram Desktop's `DocumentSaveClickHandler` saves a clicked document
under `Core::App().settings().downloadPath()`. When "Ask where to save"
is on, it opens a save dialog instead. Quill now does the same.

## What changed

- **When a copy is made.** It is made only for a download the user started
  from a document row (`request_document_download`), and only when a
  download folder is set or "Ask where to save each file" is on. With
  neither, the file stays in the cache and nothing changes. The setting is
  read again when the download finishes, so a change made while it ran
  takes effect.
- **The stages** live in `state/domains/media/folder_saves.rs`, with one
  enum per file id: `Wanted` (requested) → `Due` (TDLib reported it
  complete with a usable path) → `Saved { dest }`.
  - A cancel removes `Wanted`, so a cancelled file is never copied.
  - `take_due` removes a file while it is being copied, so a second
    completion update for the same file finds nothing to do.
  - A request that arrives during the copy is kept, not overwritten by the
    copy's result.
- **The copy** is in `ui/folder_saves.rs`. It goes to the folder, or
  through a save dialog when "ask" is on, using
  `media_viewer::save_file_in_dir_as`. The copy takes the document's own
  name (`document.file_name`), not TDLib's cache name.
  - The name is made safe on every OS: path separators and characters
    Windows rejects are replaced, and `..` cannot leave the folder.
  - A name already in the folder becomes `Name (2).pdf`, the same `(n)`
    de-duplication the viewer uses.
  - When the copy succeeds, the "Saved to …" note appears.
- **Open, Show in folder and the Downloads list** use the copy while it is
  still on disk, and the cache file otherwise.

## Left out

- Where a copy went is kept only in memory. After a restart, Open uses the
  cache file again (the copy is still in the folder).
- If the folder cannot be reached, for example a cloud drive that is not
  mounted yet, it counts as unset and the file stays in the cache. No
  error is shown.
- Downloads started by auto-download, voice notes, videos and photos are
  not copied. Only a document the user clicked is, which is the case
  `DocumentSaveClickHandler` covers.

## Testing

- Unit tests:
  - `state::domains::media::folder_saves` has four tests: a wanted document
    is due once; a cancelled or unrequested file is not copied; a request
    made during a copy survives; the saved copy is remembered and a new
    request copies again.
  - `media_viewer::save` has one test: names are sanitised, kept inside the
    folder and de-duplicated.
- By hand, by the contributor, on Windows: the portable build of this
  branch (built from tag `v2026.1119.6880` on the fork) with the download
  folder set to a folder on a Google Drive mount (`G:`). Documents
  downloaded from a chat landed in that folder.
