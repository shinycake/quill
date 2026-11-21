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

- **When a copy is made.** Only for a download the user starts from a
  document row (`request_document_download`), and only when a download
  folder is set or "Ask where to save each file" is on. With neither, the
  file stays in the cache and nothing changes.
- **Where it goes is decided at the click**, as in Telegram Desktop:
  `SaveTarget::Folder { dir, name }` for a set folder, or
  `SaveTarget::Exact(path)` from the save dialog, which opens at the click
  when "ask" is on. A closed dialog downloads nothing. The click does no
  filesystem work: the folder is checked when the copy runs, off the UI
  thread, and one that cannot be reached counts as unset.
- **The target travels with the download intent**
  (`DownloadIntents::save_to`, set by `ConnectDriver::download_user_file_to`
  only when a request went out). `put`, the intents' single writer, drops
  it with any change that is not a running user download, so a cancel, a
  failure, a removal from the list (here or on another device) or the
  session closing leaves nothing to copy later.
- **A finished target** is queued in `media.folder_saves` and drained
  from the poll (`poll_live`). The UI owns the two stages that run there,
  `FolderCopy::Asking` and `FolderCopy::Copying`, each holding its task
  (`ui/folder_saves.rs`, an `OnScreen` field): a newer copy of the same
  file or the account leaving the screen drops the older task.
- **The copy** takes the document's own name (`document.file_name`), made
  safe on every OS: path separators, characters Windows rejects and bidi
  controls (U+202E and friends, which can show `…exe.pdf` for an `.exe`)
  are replaced, and `..` cannot leave the folder. A name already there
  becomes `Name (2).pdf`. When the copy succeeds, the "Saved to …" note
  appears; a failure shows an error and is logged.
- **Protected content is not copied out**: a chat with protected content,
  or a self-destructing or one-time message, downloads to the cache only,
  the same rule Save As and the viewer's Save follow.
- **TDLib's cache copy stays.** An earlier version of this branch deleted
  it, but every surface that reads the file map (the message menu, the
  viewer, shared media, the Downloads panel) then treated the document as
  never downloaded. Open and Show in folder use the saved copy while it is
  on disk, and the cache file otherwise.

## Left out

- Where a copy went is kept only in memory, so after a restart Open and
  Show in folder use the cache file.
- Downloads started by auto-download, voice notes, videos and photos are
  not copied. Only a document the user clicked is, which is the case
  `DocumentSaveClickHandler` covers.
- A finished download of an account that is not on screen is copied when
  that account is back on screen.

## Testing

- Unit tests: `state::domains::media::folder_saves` (the queue and saved
  copies), `state::tests::downloads` (a target is due once on finish; a
  cancel, failure or removal copies nothing later; only a running user
  download takes a target), `media_viewer::save` (names kept inside the
  folder and de-duplicated) and `export::names` (bidi controls).
- By hand, by the contributor, on Windows with the download folder on a
  Google Drive mount (`G:`): documents downloaded from a chat landed there.
