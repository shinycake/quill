# Viewer chrome keeps clear of the Dock and taskbar; the controller drags

## Problem

The media viewer draws inside the window it was opened from. When that
window reaches under the Dock (macOS) or the taskbar (Windows, Linux
panels), the caption, seek bar and controller sat 16 pt off the window's
bottom edge, under the Dock. tdesktop never meets this: its viewer is a
window of its own, maximized to the screen's available area
(`OverlayWidget::updateGeometry`), with the controller 6 px off its bottom
(`mediaviewCaptionPadding.bottom`).

## Decision

- **Cover, not a new window.** `quill::media_viewer::screen_cover` works
  out how far in from each edge the screen hides the window: the window's
  frame against its screen's work area. The viewer's chrome (top bar,
  arrows, caption, controller, album strip) and the picture's fit area
  keep to what is left (`ScreenCover::safe_area`). Resizing or moving the
  user's window to open a picture would be worse than drawing around the
  Dock.
- **Where the work area comes from** (`ui/media_viewer/screen_cover.rs`):
  macOS `NSScreen.visibleFrame` and Windows `MONITORINFO::rcWork`, both
  through GPUI's `visible_bounds`; X11 `_NET_WORKAREA` of the current
  desktop against the window's place on the root window, read on Quill's
  own X connection (`window_control::x11_frame_and_work_area`, cached per
  window and bounds). Wayland gives a client neither, so nothing is
  covered there; compositors keep panels off maximized windows.
- **Slivers are ignored.** Covers under 12 pt are dropped: the 4 pt strip
  an auto-hidden Dock keeps, and the 7 px invisible border Windows counts
  in a frame. No edge counts for more than 40 % of the window, so a window
  hanging off the screen still has room for its chrome. A frame that does
  not meet the work area (another screen's) gets no cover.
- **Full screen** has no cover: the Dock and taskbar only slide over the
  window while the pointer asks for them.
- **Higher by default.** The bottom bar's gap is 32 pt (was 16), in full
  screen too, so the controller is off the edge where a hidden Dock slides
  in.
- **The controller drags** (`ControllerPlace`): a press on the panel's
  background (not a button, the seek bar or the volume slider) picks it up
  and it follows the pointer inside the safe area, 8 pt in from its edges.
  Within 24 pt of home it snaps onto it. Away from home the bar shows a
  "Move controls back" button (and a double-click on the panel does the
  same) that puts it home from anywhere. The spot lives with the account's viewer
  state, so it stays moved from one video to the next, and is kept inside
  the window as the window resizes.
- **The caption goes with it.** Away from home, the caption leaves the
  bottom bar and sits on top of the controller in one bar of the
  controller's width (one translucent plate; a press on either part drags
  it), and the picture takes the caption's old room. There it is folded to
  a one-line chip (`DockedCaption::Folded`) that opens the whole caption on
  a click and folds back from a chevron; each item starts folded. Back home, it returns
  under the picture. The caption plate is translucent and rounded like the
  controller's in both places.

## Verified

Screenshot demos (`ready-video-controls`) with
`QUILL_DEMO_SCREEN_COVER=top,right,bottom,left` standing in for a Dock,
and the new `dc:x,y` double-click step: before/after, a 70 pt bottom Dock,
a drag mid-flight, release with the caption docked, snap back, double-click
home, and a seek-bar drag that does not move the panel. Unit tests cover
bottom, left and right docks, an auto-hidden Dock, Windows' invisible
border, a second screen, a window hanging off the screen, and the drag.

## Not verified

A real Dock, taskbar or Linux panel over a real window, a Dock moved
between screens, and the X11 path on a live X server (it was cross-checked
to compile with `clippy --target x86_64-unknown-linux-gnu`).
