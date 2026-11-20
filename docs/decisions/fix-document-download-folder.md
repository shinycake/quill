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
- **The cache copy is deleted** once the copy is written, so the document
  lives only in the folder.
  - Quill sends `removeFileFromDownloads(delete_from_cache: true)` for a
    download in TDLib's list, then `deleteFile` for one that is not
    (`ConnectDriver::delete_cached_copy`, purpose `DeleteCachedCopy`).
    Answers are not waited on, and a failure leaves the cache file.
  - Documents Quill opens as pictures keep their cache copy (`keep_cache`,
    from `opens_as_picture`), because the viewer reads them from TDLib's
    files directory.
  - Saving the same document again downloads it again and writes a new
    copy, de-duplicated with ` (n)` even when the files are identical.
- **Document rows and the Downloads list** count a document with a saved
  copy as downloaded (`FolderSaves::has_copy`), since TDLib no longer
  reports it complete. Rows show Open, and the Recent section keeps it.
- **Open and Show in folder** use the copy while it is on disk, and the
  cache file otherwise. If the copy has been deleted from the folder,
  Open downloads the document again.

## Left out

- Where a copy went is kept only in memory. With the cache copy deleted,
  the row shows Download again after a restart. Downloading it writes a
  new copy (`Name (2).pdf`) next to the first. This was chosen over
  keeping a per-account record of copies.
- If the folder cannot be reached, for example a cloud drive that is not
  mounted yet, it counts as unset and the file stays in the cache. No
  error is shown.
- Downloads started by auto-download, voice notes, videos and photos are
  not copied. Only a document the user clicked is, which is the case
  `DocumentSaveClickHandler` covers.

## Testing

- Unit tests:
  - `state::domains::media::folder_saves` has five tests: a wanted document
    is due once; a cancelled or unrequested file is not copied; a request
    made during a copy survives; the saved copy is remembered and a new
    request copies again; a picture keeps its cache copy, and a saved copy
    counts as downloaded.
  - `media_viewer::save` has one test: names are sanitised, kept inside the
    folder and de-duplicated.
- By hand, by the contributor, on Windows: the portable build of this
  branch (built from tag `v2026.1119.6880` on the fork) with the download
  folder set to a folder on a Google Drive mount (`G:`). Documents
  downloaded from a chat landed in that folder. That build came before
  the cache copy was deleted, and that step has not been tested by hand
  yet.
