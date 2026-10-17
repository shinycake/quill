# viewer-zoom-levels: zoom steps and Ctrl+0 in the media viewer

## Problem
The viewer's `+` and `-` keys and the toolbar buttons multiplied the zoom by 1.15 between fit and
eight times fit. A step never landed on the picture's real size, it could not go below fit, and
the label counted percent of the fit size, so 100% meant "fits the window" rather than "one
picture pixel per point". Ctrl+0 did nothing in the viewer.

## tdesktop
`OverlayWidget` (media_view_overlay_widget.cpp) keeps an integer `_zoom` level. Level z at or
above 0 shows the picture at z + 1 times its size, a level below 0 at 1 / (1 - z), and
`kMaxZoomLevel = 7` bounds both ends. One fractional value, `_zoomToDefault` (the scale that
fits the picture in the window between the bars), is stored as the special level
`kZoomToScreenLevel`. `zoomIn` and `zoomOut` move one integer level, except that they stop on
that fit scale when it lies between two levels, and from it they go to the next integer level.
`zoomUpdate` skips levels that would shrink the picture below a pixel.

`zoomReset` (the `0` handling behind Ctrl+0 in `filterApplicationEvent`, and the middle mouse
button) goes from level 0, the natural size, to the fit scale, and from any other zoom to level
0.

tdesktop uses its second scale, `_zoomToScreen`, only for full-screen video. It fits the picture
in the whole screen, so it is another fit, not a cover. Its wheel and pinch step through the same
levels.

## Quill
- `quill::media_viewer::ZoomLevels` is the ladder of one picture in one fit area: the 15
  integer levels, minus the ones that would leave the picture under a point on a side, plus the
  fit snap. `step` returns the nearest stop above or below the current scale, and the last stop
  at either end, so a fit beyond the level limits stays reachable as the outermost stop. `reset`
  is `zoomReset`.
- `ViewerZoom` keeps its zoom as a factor of the fit size, so the geometry code did not change.
  It now carries its `ZoomLevels`, which `viewer_geometry` rebuilds when the media box or the
  ladder changes, and the zoom resets to fit as before. `percent` is what the label shows: the
  zoom against the natural size, or against fit while the natural size is unknown. With the
  ladder, 100% is one picture pixel per point, as the levels count it.
- Below fit the picture sits centered in the media box, and dragging does nothing there.
- Cmd/Ctrl+wheel and pinch stay continuous, as #929 made them, inside their old range of 1x to 8x
  fit. After the keys stepped outside that range, the wheel only moves back toward it, so it
  never jumps. A `+` after a wheel zoom goes to the next stop above the current scale.
- `0` (the rebindable `viewer-zoom-reset`) and Ctrl+0 (Cmd+0 on macOS) run `toggle_natural`:
  natural size from anywhere but natural size, and fit from there, centered as `zoomReset`
  centers it. The Reset button and a double-click still go straight to fit. Video full screen
  keeps its zoom, as tdesktop's `zoomReset` returns early there.
- Ctrl+0 is `ViewerKeyAction::ResetZoom`, read by the viewer's keystroke interceptor ahead of
  the app's bindings. It is not a default of the rebindable action: Cmd/Ctrl+0 is the fixed
  Open Saved Messages binding, and `resolve_keybindings` never installs a rebindable chord that
  fixed chrome owns. The interceptor only sees keys while the viewer is open, so the chord opens
  Saved Messages again once the viewer is closed.

## Differences from tdesktop, on purpose or not yet
- tdesktop closes the viewer at once (`OverlayWidget::hide`; `mediaviewHideDuration` is the
  controls' fade), and Quill does the same.
- A zoomed picture stays clipped to the fit-sized media box, as every Quill zoom is. tdesktop
  draws a zoomed picture over the whole window. Letting the zoom spill out of the media box is a
  separate change to the viewer's geometry.
- tdesktop's wheel and pinch step through the levels. Quill keeps them continuous.
- tdesktop's default zoom shows a picture smaller than the window at its natural size. Quill
  fits every picture, so a small one opens above 100%.

## Tests
- `media_viewer::transform` covers the level scales, fit landing on a level, stepping up from
  fit to natural size and back, the -7 and +7 limits, a fit beyond them, a tiny picture's floor,
  `zoomReset`, centering below fit and a wheel below fit.
- `media_viewer::controls` covers Ctrl+0 on every item kind, and plain `0` staying the
  rebindable reset or the full-screen seek.
- `zoom_ui_tests.rs` drives the real app: `+` from fit reaches natural size and then level 1 on
  a photo larger than the window, the label follows, `-` steps back past fit, `0` and
  Cmd/Ctrl+0 toggle fit and natural size without the chord leaking to Saved Messages, and
  Cmd+wheel still moves one continuous step.
