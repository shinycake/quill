# Thumbnail strip for profile photo galleries

## What tdesktop does

`OverlayWidget::refreshGroupThumbs` (media_view_overlay_widget.cpp) feeds
`GroupThumbs::Refresh` from whichever slice the viewer pages over. Two of
them cover profile photos:

- `UserPhotosSlice` (a user's photos). `ComputeContext` returns the user's
  peer for every index, so the whole loaded list is one group.
- `SharedMediaWithLastSlice` over `SharedMediaType::ChatPhoto` (a group's
  or channel's photos). The isolated current photo's context is
  `photo->peer`, and each photo-change service message's context is the
  history's peer, so that list is one group too.

The strip then shows as many thumbs as `ComputeThumbsLimit` allows on each
side of the current one, which is the same layout as an album. Each thumb
asks for `PhotoSize::Thumbnail` of its photo (the `"m"` size). Clicking a
thumb opens that photo, and the strip slides on every step.

The slice keeps `kIdsLimit` (48) photos loaded around the open one. When
fewer than that are loaded past it, the next page is requested.

## What changed

- `ViewerGroup::ProfilePhotos`: `profile_photo_items` puts every gallery
  item in it. `group_strip` and `strip_frames` then handle profile
  galleries with no other changes: the layout, the slide, and the click
  that jumps to a photo are the album strip's.
- `thumb_file_ids`: a profile thumb draws the photo's small size (the
  `"m"`-or-nearest file `parse_profile_photo` picks), with the full size
  as a fallback.
- `MediaViewer::strip_downloads` lists the small sizes the strip shows.
  `download_viewer_strip_thumbs` sends them through the driver's
  `download_file` at thumb priority. It runs on open, on every step, and
  from the poll loop's gallery sync, never from render. The width it lays
  the strip out at is the lane the last frame measured
  (`ViewerExtra::strip_lane`), the same kind of measurement as
  `viewer.frame`.
- Profile paging now asks for the next page while fewer than
  `VIEWER_PROFILE_PHOTOS_AROUND` (48, tdesktop `kIdsLimit`) photos are
  loaded past the current one. Before, the limit was 6. Pages that arrive
  extend the strip.

Album strips are unchanged. They still draw the files their chat already
loaded.

## How it was verified

- Unit tests (`group_thumbs.rs`): a gallery is one strip with the 15/6
  point gaps; a lone photo has none; a long gallery shows what fits and
  grows when the next page lands; thumbs pick the small size; the strip
  downloads only the thumbs it shows, and none for albums.
- `mod.rs` paging test, updated for the 48-photo margin.
- GPUI test (`group_thumbs_ui_tests.rs`): in the `ready-info-group-photos`
  demo, three thumbs show. Clicking the third opens "Photo 3 of 3" and
  slides it to the centre. Stepping back makes the second thumb current.
- `/verify` captures of the same demo in light and dark, before and after
  clicking the third thumb.

The captures could not show live thumb downloads or paging past 100
against a real server, because the demo's files are already local.
