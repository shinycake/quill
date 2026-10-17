# Shared media: thumbnail grid for photos, videos and GIFs

## What Telegram Desktop does

`Info::Media` lays the Media, Photos, Videos and GIFs tabs out as tiles under month headers
(`info/media/info_media_list_section.cpp`, `overview/overview_layout.cpp`: `Photo`, `Video`,
`Gif`). A tile paints the inline blur at once, then the thumbnail (`Photo::paint` :450,
`Video::paint` :712); videos carry their length (`Video::updateStatusText`), GIFs a "GIF" mark
(`StatusText::update` :371).

## What Quill did

All tabs but Stories and Gifts were glyph-and-label rows.

## What changed

- `SharedMediaMeta::Tile` (src/state/shared_media_meta.rs) holds a photo, video or GIF's
  thumbnail file ids (best first), size, minithumbnail and badge (`TileBadge::Duration` or `Gif`).
- Parsing a `GetSharedMedia` page queues those thumbnails through the existing
  `profile_thumbs_due` path, which the connect layer downloads (data saver respected).
- `src/ui/shared_media_grid.rs` draws month headers over wrapped 96 px tiles: the minithumbnail
  until the downloaded thumbnail exists, then the thumbnail through `image_budget::sized_media`.
  A click opens the media viewer over the tab (as the rows did); Go to message stays in the
  context menu. Paging ("Load more") and the calendar's date bar are the list's.
- The tile grid shares nothing with the Stories/Gifts tiles beyond size: those draw story and
  gift visuals, these draw message thumbnails.

## Deviations

- Tiles are a fixed 96 px (three per row at the panel width), not tdesktop's justified rows that
  keep each picture's aspect. Grid zoom is not part of this change.
- Round videos have no tab of their own in Quill; they stay rows in the Voice tab.
- Spoiler and secret media draw as a plain tile (no thumbnail, no blur) instead of tdesktop's
  spoiler overlay.

## Verified

Unit test `photo_and_video_pages_become_tiles_and_queue_their_thumbnails`; captures of the
Media and GIFs tabs in light and dark (`ready-shared-media`, `QUILL_DEMO_SHARED_MEDIA_TAB=media|gifs|videos`).
