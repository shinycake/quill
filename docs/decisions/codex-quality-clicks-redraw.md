# Nested clicks, story chips and the viewer's video redraw

## What tdesktop does

- A press on a `ClickHandler` (a link, a story area) suppresses the action of
  the widget around it (`OverlayWidget::handleMousePress`: `ClickHandler::pressed()`,
  then only `if (!ClickHandler::getPressed())` the video/hold actions start).
  Story areas therefore never pause the story.
- Muted GIFs, stickers and loops stop behind another app (`isGifPausedAtLeastFor`);
  video with sound keeps playing.

## What changed

- `SwallowPress::swallow_press()` (`src/ui/nested_click.rs`): a left mouse-down
  that stops propagating. Kit buttons do not stop the press, so a button inside
  a clickable parent fired both. Sites now swallow the press on the nested
  control; GPUI then never records it on the parent. Several older sites
  called `cx.stop_propagation()` from the `on_click` handler instead; that also
  works, but the press stop is the one pattern now, behind a helper.
- Real bugs fixed (nested control also fired the parent): album video "Play"
  (opened the viewer too), thread-info toggle (jumped to the root message).
  The composer schedule-chip clear had no stop at all in its press path
  either (it stopped in the click handler only).
- Moved to the same helper (they already stopped from the click handler):
  call bar mute/hang up, group call bar mute/leave, group call tile
  full-screen button, GIF and video play discs. Existing mouse-down stops
  (profile modal, menu audience delete) were already right.
- Story area chips swallow the press, so tapping one no longer pauses and
  resumes the story.
- The media viewer's native video no longer calls `request_animation_frame`
  every frame. It asks the frame clock (`request_media_tick`): 60 fps while the
  window is active, 10 fps in the background (the sound keeps playing).

## Verification

- `nested_click` UI tests: a button inside a clickable parent fires both without
  the helper, and only itself with it (including that a later click elsewhere
  does not replay the parent).
- Gate: `gate.sh` prints GATE OK.

Not verified: `QUILL_TRACE_TICKS` traces with a playing video, and a real video playing in a backgrounded viewer window (no
interactive session in this run).
