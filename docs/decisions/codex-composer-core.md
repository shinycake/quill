# Composer core: drop zones, folder archives, code language, large emoji

Cluster `composer-core`. Three items landed in full and five were left alone on purpose (see the end).

## Drop zones and folder archives (`composer-drop-modes`)

In tdesktop: `DragArea` (history/history_drag_area.cpp) watches a file drag over the chat and asks `ComputeMimeDataState` what it holds. A lone document gets one zone, "Drop files here / to send them as documents". Photos get two: "Drop images here / to send them without compression" on top and "Drop photos here / to send them in a quick way" below. Photos and videos, several documents, and a single folder get their own pairs; a folder offers "to send all its files" or "to send it as an archive". Several files can go as one archive. `storage_folder_archive.cpp` builds the zip: the folder name is the root, symlinks are skipped, already-compressed formats are stored, the rest is deflated, and duplicate names become "name (2).ext".

In Quill before: Dropping files anywhere on the composer attached them as one auto-chosen kind. There was no choice, and a folder was rejected.

Now:
- `src/drop_modes.rs` classifies a drag (`classify`), returns the zones and their wording (`zones`), and turns the zone that got the drop into an attach list, an archive job or a refusal (`plan_drop`). `folder_files_for_sending` matches tdesktop: the folder's files, then the files of each direct subfolder, sorted.
- `src/folder_archive.rs` writes the zip itself, streaming each entry through a data descriptor so nothing is held in memory and the 2 GB upload limit is checked as bytes are written. It uses `flate2`, which was already a dependency, so `Cargo.toml` is untouched. Archives go to `<media cache>/archives/<unique>/<name>.zip`; folders older than a day are removed on the next archive.
- `src/ui/drop_zones.rs` shows the zones over the whole conversation while a drag is over it. GPUI delivers the same `ExternalPaths` drag on macOS, Windows and Linux, so one code path covers all three. Each zone highlights while the pointer is over it. A drop outside the zones, or while editing a message, keeps the old behaviour (single file replaces the media when editing). The archive is built on a background task; if the person switches chats meanwhile, the result is deleted instead of attached to the wrong chat.
- `ComposerAttachment::append_planned` attaches files with a kind already chosen. `append_dropped_files` now goes through it.

Not copied: tdesktop promotes the lower zone to "archive" when a modifier key is held; Quill has no equivalent yet.

## Code language (`composer-code-language`)

In tdesktop: Clicking a code block's button in the field opens `EditCodeLanguageBox`: title "Code Language", label "Language for syntax highlighting.", an empty field showing "Auto-Detect", at most 32 characters of letters, digits, `+` and `-`.

In Quill: The composer keeps fenced blocks as text, so the language is the word after the opening fence. `src/code_language.rs` finds the block around the caret with the same rules the parser uses, validates the name, and rewrites the fence. The Formatting submenu has "Code Language…", enabled when the caret is inside a block. It opens the same box above the input row, with a counter, an inline error, Save and Cancel; Esc closes it. If the draft changed while the box was open, nothing is edited.

## Large emoji (`settings-large-emoji`)

The renderer already enlarged one to three emoji (PR "Big emoji"), with a switch under Media settings. tdesktop keeps "Large emoji" in Chat settings, so the switch moved to the Emoji group of Appearance, next to "Suggest emoji replacements". The preference and the rendering are unchanged.

## Not done

- `composer-up-edit-media`: tdesktop's Up key opens the caption box for a local (still uploading) media message. TDLib cannot edit a message that has only a temporary id, so there is nothing to edit; Up on a sent media message already works through edit-last.
- `composer-restricted-placeholder`: needs the member's own restricted rights from `chatMemberStatusRestricted`, which Quill does not store, and `chat.permissions` holds only the group defaults. Guessing from the defaults would block administrators.
- `composer-voice-pause`: pause, resume and preview are doable, but "Play once" needs view-once voice messages end to end (send and render).
- `composer-custom-emoji`: drawing custom emoji inline in the text field needs a change to the input widget in `third_party/gpui-base`.
- `composer-send-options`: four separate send-box features (HD photo, GIF with caption, paid media price, video cover); none is built.

## Verification

- Unit tests: classification table, zone wording, zone-to-plan mapping, folder listing, zip round trip (names, methods, CRC, contents), duplicate naming, stored versus deflated, size limit and cleanup, DOS timestamps, symlink skipping, fence lookup, language validation and rewriting, and that the rewritten fence parses to a Pre entity with that language.
- A zip made by the writer passes `unzip -t` and Python `zipfile.testzip()`.
- Demo captures `ready-drop-zones`, `ready-drop-folder` and `ready-code-language` (English fixtures) and `ready-appearance` were rendered and inspected.
- Not exercised: a real OS drag onto the window on any platform (the capture shows the zones with a fixed state), and a live upload of an archive.
