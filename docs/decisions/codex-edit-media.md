# B5: edit media, caption position, link-preview options

## What tdesktop does
- `boxes/edit_caption_box.cpp`: "Replace attachment" (`ChooseReplacement`) takes exactly one file. Stickers (webp/tgs) are refused with "Sorry, no way to use this file." Inside an album the file must fit the album type (`canBeInAlbumType`): photo/video albums take photo or video, music albums take audio, file albums take documents; otherwise "This file cannot be saved as a part of an album." Outside albums, "Send as a document" toggles compressed vs file (`CanToggleCompressed`). Spoiler is offered for photo/video. `HistoryItem::allowsEditMedia`: photo, video, GIF, document, music; not voice, video note, sticker or self-destructing media.
- Drag/paste of a single file or image while editing also replaces the media (`ValidateEditMediaDragData`).
- `history_view_draft_options.cpp` ("Link Preview Settings"): click a link to pick which one generates the preview, Move Up/Down, Shrink/Enlarge Photo/Video, Do Not Preview.

## What changed
- Core: `ComposerEdit` carries `MediaEdit` (current media kind, album membership, staged `EditMediaReplacement`) and a `LinkPreviewChoice`. `replacement_for` implements the rules above. New requests `editMessageMedia`, `inputMessageAudio`/`inputMessageDocument` builders; `editMessageText` now sends `linkPreviewOptions` (shared `link_preview_options_value`, also used by sends, plus a `link_index` that pins the chosen URL). `edit_snapshot` sends `editMessageMedia` when a replacement is staged (caption, caption position and spoiler ride in the content), else `editMessageCaption` as before.
- UI: edit banner gets a Replace button (file dialog on all OSes; drop and paste also replace), a replacement chip (as-document, spoiler, keep original), caption-position checkbox only for photo/video/GIF (including after replacement). The link chip now has one "Link options" popover (also in text edits, seeded from the message's own preview position).
- Caption position toggle already existed for sends and caption edits; now hidden for documents/music.

## Skipped
- Adding media to a plain text message (no TDLib editMessageMedia on text), Edit Image/Video/Cover, up-arrow on a pending media message, replacing scheduled messages' media, audio title/performer (left to the server).
- Demo edits apply caption position locally but not the replaced file.

## Verified
Gate (fmt, clippy, core and UI tests): new unit tests for the rules, request shapes and the driver edit path. `ready-edit-media` capture shows Replace, the replacement chip and the caption-above toggle. Not verified live (no account); the link popover was not captured since menus are not capturable.
