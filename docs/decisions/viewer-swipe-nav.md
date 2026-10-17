# viewer-swipe-nav: horizontal trackpad swipe in the media viewer

## Problem
A horizontal two-finger swipe over the viewer did nothing. The wheel reducer drops
horizontal-dominant scrolls, so on a trackpad the only ways to the next photo were the arrows,
the keyboard, or a vertical scroll, which is not where a finger goes to flip a photo.

## tdesktop
`OverlayWidget::setupSwipeNavigation` (media_view_overlay_widget.cpp) hooks the viewer body
into `Ui::Controls::SetupSwipeHandler` (swipe_handler.cpp). For wheel events with phases:

- The gesture locks horizontal once the summed horizontal delta beats the vertical one by more
  than 1, and vertical when it is the other way round. A vertical gesture is left alone.
- The swipe direction is fixed by the first horizontal movement. A swipe toward an end with no
  item (`_leftNavVisible` / `_rightNavVisible` false) is treated as vertical.
- `ratio` is the pull over a 50 px threshold. `RatioRange` slides the threshold window with the
  pull, so backing off by one threshold from any overshoot cancels.
- Reaching ratio 1 arms the gesture with a haptic tap and a short bounce. Dropping below 0.95
  disarms it.
- An end phase, a momentum event or a new begin phase ends the gesture. Ending armed moves one
  item.
- While a swipe runs, `_swipeNavigating` makes `handleWheelEvent` return early, so the wheel
  rule never sees it.
- `SetupSwipeBack` draws the arrow: a 150 px box whose circle is the box less a quarter on each
  side, a chevron in the visible half, opacity equal to the ratio. The box slides from just
  outside the edge to half visible.
- Stories have their own swipe handling. Zoomed-in pictures still page, since dragging to pan
  uses the mouse button.

## Quill
- `quill::media_viewer::SwipeNav` is a pure reducer next to `WheelNav`. `feed` takes one
  `SwipeInput` and returns `SwipeAction` (`None`, `Capture`, `Step`). `end` handles the end
  phase and the timeout, and `indicator` is what the UI draws. `viewer_scroll_wheel` feeds it
  first and returns before `WheelNav` on `Capture`, which is Quill's `_swipeNavigating`.
- Only pixel deltas (trackpads and smooth-scrolling mice) swipe. Mouse wheel lines never do.
- The orientation lock, the fixed direction, the sliding threshold window and the 0.95 disarm
  follow tdesktop. A gesture that leans neither way yet stays undecided and keeps adding up its
  deltas, so the zero-delta first event macOS sends does not decide anything.
- Releasing armed moves one item. Releasing below the threshold drops the arrow and moves
  nothing. After a release, the rest of the gesture (macOS momentum arrives as plain moved
  events) is swallowed until a new begin phase or a 300 ms pause, the same rule `WheelNav` uses.
- Gestures without phases (Wayland and X11 smooth scrolling, Windows drivers outside Direct
  Manipulation) never get an end phase. They end 300 ms after their last event, from one timer
  task spawned when the swipe starts. It sleeps until the deadline, and when an event arrived in
  the meantime it sleeps again until the new one. A gesture that began with a phase waits for its
  end phase however long the fingers rest, as in tdesktop, and a new begin phase finishes one
  that lost its end.
- Showing another item by any route drops a pull in progress.
- The arrow is a pure function of `SwipeNav::indicator`, drawn in the chrome slice
  (`swipe_indicator.rs`): a 75 px dark grey circle with a white chevron, opacity equal to the
  ratio, sliding from just outside the edge to half visible, vertically centred on the picture
  like the arrows. It has no hitbox. Once armed it bounces once, up to a quarter larger and back
  over 250 ms (tdesktop's bounce part of its reach animation), through a GPUI animation that
  starts each time the gesture arms.

## Tradeoffs
- tdesktop fills the circle with its translucent black control colour over a translucent grey
  backdrop. Quill's backdrop is solid black, where that circle cannot be seen and only the
  chevron shows. Quill uses a dark grey at 85% instead, which shows on black and still keeps
  the white chevron readable over a bright picture in video full screen.
- tdesktop also draws an expanding ring when the gesture arms and taps the haptic engine. Quill
  only bounces the circle. GPUI has no portable haptic call.
- tdesktop's wheel path scales the scroll delta by 0.2 and its back-swipe threshold by 0.35, so
  a trackpad needs about 88 px of travel there. Quill uses a plain 50 px. That is shorter, and
  the travel can be tuned with the one constant.
- Pulling back across the start does not flip the direction. It only slides the threshold
  window down, as in tdesktop, so the swipe has to go the original way again to arm.
- The 300 ms timeout only applies without phases. A phased gesture whose end phase is lost keeps
  its arrow until the next scroll event or until the item changes.

## Verification
- `cargo test --lib media_viewer::swipe`: short swipes cancel, a full swipe steps once,
  momentum after the release does not step again, a vertical-first gesture is not captured, no
  next item means no capture, and a swipe without phases ends after the pause.
- `cargo test --features demo-capture --bin quill swipe`: in the real viewer, a 60 px swipe
  moves photo 1 of 3 to photo 2 and the arrow is gone after, a 30 px swipe cancels with the
  arrow shown at the right edge at 40 px, a vertical-first gesture still moves by the wheel rule
  with no arrow, and a swipe without phases moves once the clock passes the pause.
- Demo captures of `ready-showcase` with the `viewer` showcase and scripted `w:` steps: the
  arrow half pulled in on photo 1 of 3, then photo 2 of 3 with no arrow after the release.
