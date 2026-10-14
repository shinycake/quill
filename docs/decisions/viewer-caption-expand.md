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

Now the viewer keeps tdesktop's quarter-window cap and adds the stories
expansion on top, as the owner asked:

- `src/ui/media_viewer/caption.rs`: the caption box is clipped to whole
  lines within a quarter of the window. When the text is taller, a ghost
  `Button` "Show more" sits under it on the right. It expands the box
  upward to the full text, or to the room between the top bar and the
  bottom controls, where it scrolls. "Show less" (or Escape) collapses it.
  Another item, or the viewer opened again, starts collapsed. In video
  full screen Escape leaves full screen first, as in tdesktop.
- The height glides over 200 ms with sineInOut through `motion::Glide`, so
  an interrupted toggle reverses from where it is. Frames come from the
  frame clock (`request_animation_tick`), not `with_animation`. Reduced
  motion and "interface animations off" snap.
- The full height comes from the caption's own layout through a tracked
  `ScrollHandle` (bounds + max offset), so quotes, code blocks and custom
  emoji count. It is measured one frame late and redrawn until stable.

## Deviations

- tdesktop keeps regular media captions cut with no expand. Quill
  expands them like stories, because the owner asked for it.
- tdesktop has no "Show less". Quill shows it under the expanded caption
  so collapsing does not depend on knowing Escape. Escape still works.
- tdesktop draws "Show more" inside the last line's skip block and elides
  that line. Quill puts it on its own row under the text, because the
  plate is translucent and an overlay would show text through it.

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
