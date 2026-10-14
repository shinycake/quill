# Viewer caption: collapse and expand

## What Telegram Desktop does

- `media/view/media_view_overlay_widget.cpp` `refreshCaptionGeometry`: a
  media caption shows at most `_maxUsedHeight / 4`, rounded down to whole
  lines of `st::mediaviewCaptionStyle`; the rest is elided. Regular media
  has no way to read the cut part in the viewer.
- Stories use the same widget: a cut caption gets a semibold "Show more"
  (`st::storiesShowMoreFont`) at its bottom right. Clicking it opens
  `media/stories/media_stories_caption_full_view.cpp`, which grows the
  caption upward to its full height (scrolling when it is taller than the
  window) over `st::fadeWrapDuration` (200 ms, `anim::sineInOut`).
  Escape collapses it again.

## What changed

Before, Quill showed the whole caption however long it was, so a long one
covered most of the photo and the controls.

Now the viewer follows tdesktop's story caption, as the owner asked
(the first version kept the quarter-window cap; he found it far too
tall):

- `src/ui/media_viewer/caption.rs`: a caption of up to four lines shows
  in full. A longer one collapses to two lines
  (`kMaxShownCaptionLines`, `kCollapsedCaptionLines`) with a ghost
  `Button` "Show more" under it on the right. It expands the box upward to
  the full text, or to the room between the top bar and the bottom
  controls, where it scrolls with the kit's vertical scrollbar
  (`ScrollableElement::vertical_scrollbar`, shown per the theme's
  scrollbar setting). "Show less" (or Escape) collapses it. Another item,
  or the viewer opened again, starts collapsed. In video full screen
  Escape leaves full screen first, as in tdesktop.
- The height glides over 200 ms with sineInOut through `motion::Glide`, so
  an interrupted toggle reverses from where it is. Frames come from the
  frame clock (`request_animation_tick`), not `with_animation`. Reduced
  motion and "interface animations off" snap.
- The full height comes from the caption's own layout through a tracked
  `ScrollHandle` (bounds + max offset), so quotes, code blocks and custom
  emoji count. It is measured one frame late and redrawn until stable.

## Deviations

- tdesktop cuts regular media captions at a quarter of the window with
  no expand. Quill treats them like story captions (two lines, expand),
  because the owner asked for it.
- tdesktop has no "Show less". Quill shows it under the expanded caption
  so collapsing does not depend on knowing Escape. Escape still works.
- tdesktop draws "Show more" inside the last line's skip block and elides
  that line with "…". Quill puts it on its own row under the two lines
  and cuts the second line without an ellipsis: the plate is translucent,
  so an overlay would show text through it, and a button beside the text
  would change the wrap width it is measured at.

## Verified

- Unit tests for the height maths and the easing; a GPUI UI test
  (`caption_ui_tests.rs`) checks that a long caption is measured as cut,
  expands, collapses on Escape without closing the viewer, starts
  collapsed on another item, and that Escape then closes the viewer.
- Demo captures at 1200×900 of a long caption, before and after, in light
  and dark, collapsed and expanded (not committed).

## Not verified

- The animation was only checked through the timing code, not watched
  live.
