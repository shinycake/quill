# Inline @gif results load at once

## Profile (live account, Saved Messages, `@gif cat`, TDLib log at verbosity 4)

- `getInlineQueryResults` answered in 0.27 s with 50 animations.
- In the next 7 s Quill sent no `downloadFile` at all; the grid stayed grey.
- 22 of the 50 thumbnails were `thumbnailFormatMpeg4`, which the grid tried
  to draw as a still image.

## Causes

1. The driver runs the download pass after message and file updates, or
   while a sticker/GIF/emoji panel is open. The inline results answer was
   in neither list, so the panel's images waited for an unrelated update
   (a new message, any file progress). Slow when the chat was busy, never
   when it was quiet.
2. MP4 thumbnails can't be drawn as images, so those cells stayed grey
   until the full clip downloaded, which only happened for the first 24.
3. Panel files shared priority 1 with every chat thumbnail and avatar.

## What changed

- The inline results answer runs the download pass.
- `Session::inline_result_file_ids_to_download`: still images and MP4
  thumbnail loops for every result, then the first 24 full clips (≤ 5 MB),
  at priority 16 (above chat thumbnails and auto-downloads, below a user
  download). tdesktop loads the panel's GIFs the same way.
- An MP4 thumbnail becomes `preview_clip` (no still image); the cell plays
  it until the full clip is there.

## Verified

A driver regression test checks that the results answer requests the
still, the preview loop and the clips at once, at the panel priority, and
that the MP4 thumbnail is not used as an image. The live profile above was
taken before the fix; the fixed build was not run against the live
account (the account's TDLib database belongs to the installed app).
