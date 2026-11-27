# Mouse wheel notches glide

Owner request (Idan, 2026-11-27), after a Windows user asked for smooth
scrolling in issue 1507. On Windows and Linux every mouse wheel notch
arrives as `ScrollDelta::Lines`, and Quill moved the content by the whole
notch at once. Trackpads send pixels with phases and already felt smooth;
they must not change.

## What tdesktop and Web K do

Telegram Desktop does not animate the wheel. `ElasticScroll::handleWheelEvent`
(lib_ui `elastic_scroll.cpp`) scrolls to `visibleFrom + delta` at once for an
unphased event, where a notch is `angleDelta * wheelScrollLines /
(kPixelToAngleDelta * 3)`, 60 px at the default three lines. Quill's notch
is already the same distance (three lines of about 20 px), so the distance
stays and only the motion is new.

Web K leaves the wheel to the browser, and browsers glide a notch: Chromium
and Firefox move it on an ease-out curve over roughly 100 to 200 ms, add a
notch that arrives mid-way to what is left, and turn at once when the wheel
turns the other way. Quill follows that.

## What it does

- A notch (`ScrollDelta::Lines` from a pointer) covers its distance over
  150 ms (`WHEEL_GLIDE_DURATION`) with ease-out cubic. The area moves a
  step on each frame, from `Window::on_next_frame`, so the steps run
  outside the draw, the way the event did, and the area's own scroll
  handlers, notifications and load-more checks see each step.
- A notch mid-glide starts a new leg with what was left plus the new
  notch, so a spun wheel keeps moving and speeds up. On an axis where the
  notch points the other way, what was left is dropped.
- It ends when the leg's time is up, when a step cannot move the area (an
  edge, or a clamp after the content shrank), or when anything else sets
  the position: a jump or scroll-to-item from code, a scrollbar drag, a
  trackpad or touch scroll. Each of those calls `WheelGlider::stop`.
  Putting the reader's place back after rows changed around it (the
  history's view hold when a page lands) is not a jump: it goes through
  `ListState::keep_scroll_top` (`MessageScrollerState::hold_offset`), and the
  glide goes on from there.
- Reduced motion (the OS setting or Quill's power saving, both through
  `App::reduce_motion`) moves by the whole notch at once, as before.
- Only scroll areas glide. Every wheel event still reaches every handler
  as it did, so handlers that read notches themselves (the media viewer's
  notch to the next item, Cmd/Ctrl+wheel zoom, the map's zoom, the stories
  strip) are unchanged.

## Where it lives

The glide is one type in the vendored GPUI, `gpui::WheelGlider`
(`third_party/gpui-pre/src/wheel_glide.rs`), with a value per scroll area,
held by the area's position: `ScrollHandle` (and an untracked scroll div's
element state), `ListState`, and through them `UniformListScrollHandle`
and the kit's `VirtualListScrollHandle`. The scroll areas that take the
wheel feed it: the `div` scroll listener, `gpui::list`'s listener, and the
kit's `ScrollBounce` (`third_party/gpui-base/src/scroll_bounce.rs`), which
owns the wheel for the history (`MessageScroller`), the chat list, settings
pages and every kit `Scrollable`. `ScrollbarHandle::wheel_glider` and
`set_glide_offset` let `ScrollBounce` step the handle's own glide without
stopping it, while `set_offset` from anywhere else stops it.

The stage is one value, `WheelGlide::{Resting, Gliding}`. A frame step
carries the `since` of the glide that scheduled it and does nothing once
the glide it belonged to has ended, so a stopped glide's last step cannot
drive the next one.

## Not covered

- Horizontal scrollers that read the wheel themselves (`ScrollableMask`'s
  vertical-wheel strips, the stories strip) still move by the whole notch.
- The text input's own wheel scrolling is unchanged.

## Checked

- `src/ui/wheel_glide_tests.rs`: the curve, notches adding up, a reverse,
  a `div`, a `gpui::list` and `ScrollBounce` stepped frame by frame on the
  test clock, stops by code, trackpad and edge, and reduced motion.
- `a_wheel_notch_glides_the_history` (`open_position_tests.rs`): the real
  history eases one notch over frames, and a jump mid-glide stays put.
  `a_page_landing_mid_glide_keeps_the_glide`: an older page landing
  mid-glide does not cut the notch short (it stopped after the first frame,
  45 px in, before `hold_offset`).
- The `n:x,y,lines` demo step (`screenshot_demo_input.rs`) sends a notch in
  the real app (macOS, debug build, 1200x800). One three-line notch,
  measured from captures: the history (`ready-unread-landing`) was 18.5 pt
  in at 20 ms, 41 at 40 ms, 69.5 at 100 ms and 78 at the end; settings
  (`ready-settings`) started later (1 pt at 40 ms, 48 at 60 ms, 76 at
  100 ms, 78 at the end). Each capture is a separate launch, so the times
  are approximate.
- Not run on Windows or Linux hardware: the notch sizes there come from the
  platform crates (Windows `wheel_scroll_lines`, X11 and Wayland three
  lines) and are unchanged.
