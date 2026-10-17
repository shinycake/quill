# Shared media: thumbnail grid for photos, videos and GIFs

## What Telegram Desktop does

`Info::Media` lays the Media, Photos and Videos tabs out as a grid of square tiles under month
headers (`info/media/info_media_list_section.cpp`): as many 82 px tiles as fit the width,
stretched to fill it, 2 px apart (`resizeToWidth`, `st::infoMediaSkip`). A tile paints the inline
blur at once, then the thumbnail (`Overview::Layout::Photo::paint` :450, `Video::paint` :712);
videos carry their length. The GIFs tab is a mosaic that keeps each GIF's aspect ratio, and the
GIFs play inline (`Gif::paint`); it never draws a "GIF" label (the string at `StatusText::update`
:371 is the status text of a loaded GIF document in file lists). Only the rows in view are painted
(`ListWidget` paints the clip and unloads what scrolls out).

## What Quill did

All tabs but Stories and Gifts were glyph-and-label rows.

## What changed

- `SharedMediaMeta::Tile` (src/state/shared_media_meta.rs) holds a photo, video or GIF's
  thumbnail file ids (preferred first), size, minithumbnail and a duration badge for videos.
  Photos, videos and GIFs all carry their minithumbnail (`VideoContent` and `AnimationContent`
  gained `minithumbnail`).
- Parsing a `GetSharedMedia` page queues those thumbnails through the existing
  `profile_thumbs_due` path. The queue is deduplicated when it is taken, and under data saver it is
  emptied instead of growing.
- `src/ui/shared_media_grid.rs` draws a virtual list (`v_virtual_list`) of rows: a month header, a
  row of tiles, and the Load more row. The tiles per row and their size follow the panel width as
  in tdesktop (`GridLayout`, src/state/shared_media_rows.rs). Only the rows on screen build tiles
  and ask for pictures, so the bounded image cache can evict the rest. Scroll positions live in
  `SharedMediaGridUi` so a tab keeps its place across re-renders.
- A tile shows the minithumbnail until the downloaded thumbnail exists, then the thumbnail through
  `image_budget::sized_media`. A click opens the media viewer over the tab (as the rows did); Go
  to message stays in the context menu. The calendar's date bar sits above the list.
- Spoiler and secret media get no thumbnail and no blur.

## Deviations

- The GIFs tab is the same square grid as Photos, with static thumbnails. tdesktop lays GIFs out
  as an aspect-ratio mosaic and plays them inline. Both need an inline playback path the gallery
  does not have.
- The panel width is the shell's fixed shared-media width (`SHARED_MEDIA_PANEL_WIDTH`), so the
  tile count is computed from it rather than measured.
- Round videos have no tab of their own in Quill; they stay rows in the Voice tab.
- Spoiler and secret media draw as a plain tile instead of tdesktop's spoiler overlay.
- A video's duration pill has no play icon.
- A video or GIF whose thumbnail is an MPEG-4 clip (not a still) shows its blur only.
- Paging keeps the existing Load more button; tdesktop loads more automatically near the bottom.

## Verified

Unit tests: page parsing to tiles and the queue (`state/tests/shared_media.rs`), blur and hidden
media for videos and GIFs, and the row layout (`state/shared_media_rows.rs`). Window tests
(`ui/shared_media_grid_tests.rs`): tiles render, a click opens the viewer, and a 400-photo tab
builds only the rows on screen. Captures of the Media, Videos and GIFs tabs in light and dark
(`ready-shared-media`, `QUILL_DEMO_SHARED_MEDIA_TAB=media|gifs|videos`).
