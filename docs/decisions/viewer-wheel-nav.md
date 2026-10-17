# viewer-wheel-nav: wheel and trackpad navigation in the media viewer

## Problem
Every plain wheel or trackpad scroll over the viewer picture zoomed it. Scrolling a gallery with
the wheel zoomed the first photo instead of moving on, and a two-finger scroll zoomed by
accident. Pinch did nothing.

## tdesktop
`OverlayWidget::handleWheelEvent` (media_view_overlay_widget.cpp): a horizontal-dominant scroll
is ignored and drops the gathered delta. The vertical delta gathers in `_verticalWheelDelta`, and
each full 120-unit step zooms at the pointer when Ctrl (Cmd on macOS) is held, otherwise moves to
the next item (scroll down) or the previous one (scroll up). `handleNativeGesture` zooms on a
pinch. The handler covers the whole overlay, not only the picture.

## Quill
- `quill::media_viewer::WheelNav` is a pure reducer: one `WheelInput` in, one `WheelAction`
  out (`None`, `Zoom`, `Next`, `Previous`). The viewer owns one; a new item drops the
  gathered delta but keeps the trackpad gesture, so its momentum cannot move a second item.
- A wheel line is one notch (40 px). Trackpad pixel deltas need three notches (120 px).
- One event moves at most one item. The rest of the delta is dropped after a step.
- A trackpad gesture moves at most one item, and a gesture that zoomed does not move. A new
  gesture starts on a begin phase or after a 300 ms pause, because macOS momentum events and
  Wayland scrolls have no begin phase. Without this, one light flick with momentum skipped
  several items. A begin phase also drops the delta gathered by the last gesture.
- Cmd (macOS) or Ctrl (Linux, Windows) zooms through the existing `wheel_zoom_factor` path.
  While the picture is zoomed in, a plain scroll also zooms, so the wheel can zoom back out to
  fit. tdesktop moves to the next item there; Quill keeps the zoom gesture instead.
- Pinch is GPUI's own `PinchEvent` (macOS magnify, Wayland and X11 gestures, Windows Direct
  Manipulation), zooming at the event position by `pinch_zoom_factor`. A Windows touchpad
  that sends pinch as Ctrl+wheel zooms through the Ctrl rule.
- The picture and the black backdrop take the wheel. The top bar, the arrows and the bottom
  rows (caption, error, controls) occlude the backdrop and keep their own scrolling, so the
  expanded caption scrolls without changing the item. The padding band around the bottom rows
  lets clicks through to the picture under it but swallows the wheel, so a scroll there does
  not change the item either.
- At the first or last item the step is dropped, so the clip and the zoom are not reset.
