# Viewer: thumbnail strip for albums

## Problem

Opening a photo or video from an album showed the item alone. The only way
to see what else the album held was to page with the arrows, and nothing
said where in the album you were apart from "Photo 2 of 40", which counts
the whole chat.

## tdesktop

`media/view/media_view_group_thumbs.cpp` draws a row of thumbnails along the
bottom of the viewer for the items that share the current item's context.
In a chat that context is the message group (`item->groupId()`, an album),
for profile photos the peer, and for web page collages, paid media and
polls the one message. Service messages (chat photo changes) group by peer,
so a chat's photo history pages as a strip too.

Layout, from `media_view.style`: thumbs are 80 high (`mediaviewGroupHeight`)
with 14 above and below (`mediaviewGroupPadding`). The current thumb is
centred on the window at its picture's own width at 80 high, at most 160
(`mediaviewGroupWidthMax`), centre-cropped when wider. The others are 56
wide crops (`mediaviewGroupWidth`). `animateToLeft` / `animateToRight` place
each thumb `skip1 + skip2` from its neighbour, where a thumb's own margin is
3 (`mediaviewGroupSkip`) and the current one's is 12
(`mediaviewGroupSkipCurrent`), so the others sit 6 apart and 15 from the
current one. `ComputeThumbsLimit` caps how many show on each side to what
fits in the width left after the current thumb and two spare slots, and at
least one. The others are drawn at 0.7 opacity.

When the current item changes, every thumb animates its left edge, width
and opacity linearly over 150 ms (`kThumbDuration`); thumbs that join fade
in and thumbs that leave fade out. A click on a thumb other than the
current one opens its item. The caption and the video controller sit above
the strip (`controllerBottom = _groupThumbsTop`), and full-screen video
hides it.

## Quill

`MediaViewerItem` gains `group: Option<ViewerGroup>`, set where items are
built: `ViewerGroup::Album(media_album_id)` for album messages (photos,
videos, GIFs and files) and `ViewerGroup::PaidMedia(message_id)` for the
items of a bought paid-media album. The rest of the viewer does not look at
it.

`src/media_viewer/group_thumbs.rs` is pure and unit tested.
`group_strip(items, current, available)` walks the run of items around the
current one with the same group, cuts it to `ComputeThumbsLimit`, and lays
out each thumb (its album position, left edge from the strip's centre,
width, current or not). `strip_frames(from, to, t)` gives the thumbs at a
moment of the slide, matching thumbs by album position. A thumb that
joins or leaves the strip travels with its nearest neighbour while it
fades, as tdesktop starts a new thumb next to the one beside it.

The slide is derived from the viewer state, not from what the last frame
drew. `MediaViewer` records the item each step left (`arrived_from`, set by
`prev`, `next` and `go_to`, shifted by a Shared Media merge, cleared by a
removal) and counts the steps. `group_strip_before` lays the album out as
it stood on that item, so the render has both ends of the slide without
remembering anything. Opening, a redraw or a resize is not a step and
does not slide.

`src/ui/media_viewer/group_thumbs_ui.rs` draws the strip as the last row of
the bottom band, so it fades and hides with the other controls and keeps
the wheel the band already stops. It sits under the zoom bar or the video
controller. The row occludes, so a click on it does not close the viewer.
Each thumb is a cover crop of the item's first local
display candidate (`viewer_display_path`), decoded at strip size through
`image_budget::sized_media`, which loads off the render path. A thumb with
nothing local draws a dark placeholder. A click calls
`MediaViewer::go_to` and then the same per-item reset the arrows use. The
slide is a GPUI `with_animation` keyed on the step count, which draws
`strip_frames` at each frame. GPUI's reduce-motion setting skips it, and
demo captures draw the settled strip.

`geometry::fit_area` reserves `GROUP_STRIP_ROOM` (94) under the controls
when the strip shows, so the picture never sits under it, and the expanded
caption keeps the same room. Video full screen shows no strip and reserves
nothing.

## Tradeoffs

- Only albums and paid-media albums. The "photos of a chat" case is out of
  scope: a Shared Media list in Quill carries no peer context for service
  photos, and the profile photo viewer pages a list of its own. Both can
  set `group` later with no change to the strip.
- The strip sits under the caption and the controls, as in tdesktop, rather
  than above them. The band's own bottom padding (16) stands in for
  tdesktop's 14 below the thumbs, and the room above them is 14 (the band's
  8 point gap plus 6).
- Thumbs show whatever display candidate is local. The viewer requests no
  thumbnail downloads for the strip; an album opened from a chat already
  has its tiles local, and a missing one shows the placeholder until the
  current-item download or the chat brings it in.
- The current thumb follows `natural_size`. An item without one is as wide
  as the others.
- Thumbs carry a Button role and the item kind ("Photo", "Video") as their
  label, the same untranslated label the header uses.

## Verification

- `cargo test --lib media_viewer::group_thumbs`: 1, 2, 5 and 30 items, the
  current item first and last, the 160 cap, a tall picture, an unknown
  size, two albums in one list, the slide frames, entering and leaving
  thumbs travelling with a neighbour, and which steps start a slide.
- `cargo test --features demo-capture --bin quill -- group_thumbs`: an album
  of three shows three thumbs, the current one 128 wide and centred, the
  picture above the strip; clicking thumb 3 opens item 3 and makes it the
  wide one; a wheel over the strip or a thumb keeps the item; the arrows
  move the wide thumb; a lone photo has no strip.
- Demo captures of `ready-showcase` with `QUILL_DEMO_SHOWCASE=viewer` (the
  hikers chat's three-photo album): the strip on open, after a right arrow,
  and in the dark theme.
- The UI tests check the slide's end state only. GPUI times
  `with_animation` on the wall clock, so a mid-slide assertion would race
  the test machine; the pure `strip_frames` tests cover the frames.
