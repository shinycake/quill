# Video player controls: controller, scrubber and keys

PR 2 of 6 of the video player overhaul (`quill-tools/designs/video-player.md`,
"Controls" and build plan item 2). PR 1 (`video-player-core.md`) moved the
viewer onto the shared player and left the old controls in place. This PR
replaces them with one controller panel, a scrubber built from the base
slider parts, a restyled speed popover, waiting, download and failure states
over the picture, and the missing keys.

## tdesktop reference

From `media/view/media_view.style`, `media_view_overlay_widget.cpp/.h` and
`media/streaming/media_streaming_document.cpp`:

| What | tdesktop | Quill |
|---|---|---|
| Panel | 480 x 72, `mediaviewSaveMsgBg` (black 0.75) | `metrics::PANEL_WIDTH` / `PANEL_HEIGHT` / `PANEL_OPACITY`, `radius_lg` |
| Scrubber | `mediaviewPlayback`: line 3, seek 12 | `TRACK` 3 (`TRACK_HOVER` 5), `THUMB` 12 (`THUMB_DRAG` 14), `THUMB_FADE_MS` 120 |
| Time labels | `mediaviewPlayProgressLabel`, 12 px semibold | `text_xs().font_semibold()`, white 0.9; remaining as "−0:12" (U+2212, as tdesktop) |
| Play | 40 px, centered | `PLAY_BUTTON` 40 |
| Volume | toggle 32 at left 6, slider 75 | `SIDE_BUTTON` 32, `VOLUME_LEFT` 6, `VOLUME_WIDTH` 75 |
| Right group | 32 px buttons, right 8, skip 4 | `BUTTONS_RIGHT` 8, `BUTTONS_SKIP` 4 |
| Speed menu | `mediaviewSpeedMenu`; presets from `FillSpeedMenu` | slider 0.5 to 2.5 step 0.1; Slow 0.5, Normal 1, Medium 1.2, Fast 1.5, Very fast 1.7, Super fast 2 |
| Hide | `mediaviewWaitHide` 1100, `mediaviewHideDuration` 600, `mediaviewShowDuration` 200 | `VIEWER_WAIT_HIDE_MS` 1100, `VIEWER_HIDE_MS` 600, `VIEWER_SHOW_MS` 200 via `controls_fade_ms` |
| Waiting | `kWaitingShowDelay` 500 ms | `WAITING_SHOW_DELAY_MS` 500 |
| Speed boost | `mediaviewSpeedBoostHoldDelay` 200, `_speedBoostSpeed` 2 | `SPEED_BOOST_HOLD_MS` 200, `SPEED_BOOST_RATE` 2.0 |

Keys (`OverlayWidget::handleKeyPress` / `handleKeyRelease`): Space and K
pause, J/L long seek, arrows short seek and 0 to 9 jump to tenths in
fullscreen, `,` and `.` step a frame while paused, Alt+Enter or Cmd/Ctrl+Enter
toggle fullscreen, Esc leaves fullscreen and then the viewer. Holding Space
plays at 2x until release; `startSpeedBoost` applies even while paused, and
a tap toggles pause on release.

## What changed

- `src/ui/video_player/controls/` (new):
  - `mod.rs`: `VideoControlsState` (the seek, volume and speed sliders, hover,
    the Space hold, the waiting delay and the focus probe) replaces five loose
    fields on `ViewerUi` and three on `ViewerExtra`. It also holds the
    `VideoController` panel and the `overlay_button` variant.
  - `scrubber.rs`: `VideoScrubber`.
  - `speed.rs`: the speed trigger (a gauge icon at 1x, otherwise a "1.5×"
    pill; "2×" while boosted) and `SpeedMenu`.
  - `overlay.rs`: `PictureOverlay` for waiting, downloading, download and
    failed.
  - `metrics.rs`: every size above, with tdesktop's names.
- `src/media_viewer/boost.rs` (core, pure): `BoostState` holds the
  press / hold / release rules with tests; the waiting delay helpers live
  there too.
- `viewer_key_action` gains `HoldSpace` and `StepFrame`. Space is decided on
  release; a 200 ms hold timer starts the boost. The key-up comes through a
  `capture_key_up` on `QuillApp`'s root. When nothing is focused, the
  viewer focuses a zero-size probe on Space, so the release reaches the
  window path. Once the viewer closes, the next root render lets go of
  that focus (`release_viewer_key_probe`).
- A hold also ends when the window loses activation (the observer in
  `app_demo.rs`, not a render) and when PiP opens: in both cases the
  release goes to another window.
- Chat windows: the keystroke interceptor is app-wide and puts the
  window's slot in place first, so Space key-downs reach a chat-window
  viewer's hold logic too. The chat-window root therefore has the same
  `capture_key_up`, with its scope set, and releases its probe the same
  way.
- The scrubber reports hover in whole pixels, so a move inside one pixel
  does not redraw the viewer slice.
- The seek slider is labeled "Seek" and the volume slider "Volume". The
  label is set on `SliderState` through a small patch to the vendored
  gpui-base (`third_party/gpui-base/QUILL-CHANGES.md`, "Slider labels"),
  because neither the kit `Slider` nor the base `Slider` takes a name.
- The seek slider's range is at least one second (`seek_range`), and its
  staleness check compares against that same range. Comparing against the
  raw length rebuilt the slider and its subscription on every render for
  clips under half a second.
- Controls fade out over 600 ms and back in over 200 ms. They stay up while
  the pointer is on them or a slider is being dragged.
- The picture shows a spinner after 500 ms of waiting, a 48 px progress
  ring with the percentage while downloading (a click cancels), and on
  failure the still picture with "This video can't be played", the error and
  "Open externally". The viewer stays open in all of these. The bottom-bar
  error line is now only for non-video items.
- The viewer leaves 96 px under a video (128 with a caption) so the panel
  never covers the picture.
- Buttons use one custom variant: white on the dark panel, with a white wash
  on hover and press, in both themes. The ghost variant's light-theme wash
  put white icons on light gray. The quality trigger uses the same variant.
- The player bar's speed button opens the same `SpeedMenu`.

### A PR 1 bug fixed here

In layered mode the viewer's cached slice root was
`div().absolute().inset_0()`. The slice box gives it no height, so the root
measured 1200 x 0 and `bottom_0` pushed the bottom bar above the window. The
controls could not be seen at all. The root now takes the viewport size
(`overlay.rs`). No test caught it; the first capture of this PR did.
`the_controller_sits_inside_the_window` now covers it: in layered mode the
panel must lie inside the window, in its lower half. Run against the old
root, the test fails with the panel at y = −88.

## Why the scrubber is composed

The packaged kit `Slider` covers the volume and speed sliders. It cannot be
the scrubber:

- it has no buffered layer under the fill;
- its thumb is always visible, with no hover reveal or fade;
- it has no time bubble at the pointer;
- its track and thumb sizes are fixed (6 px track, 16 px thumb), while tdesktop
  uses 3 px growing to 5 px, and a 12 px thumb.

`VideoScrubber` is built from the base parts (`Slider`, `SliderTrack`,
`SliderIndicator`, `SliderThumb`), so it keeps the base slider's dragging,
keyboard and accessibility behavior. It adds the buffered spans, the
transitioned thumb, and a bubble (a deferred base `Tooltip` element placed
at the pointer fraction).

## Deviations

- **Speed presets.** The design lists 0.5 / 0.75 / 1 / 1.25 / 1.5 / 1.75 / 2.
  Quill uses tdesktop's `FillSpeedMenu` set (0.5, 1, 1.2, 1.5, 1.7, 2) with
  its names, because the design asks for tdesktop parity everywhere else.
- **Preset rows are kit `Button`s, not menu items.** `MenuItemElement` is
  `pub(crate)` in the kit, and a nested `PopupMenu` draws its own chrome
  inside the popover and closes on slider clicks.
- **Rate text** uses "×" (U+00D7), like tdesktop, not the design's "x".
- **Failed state** offers "Open externally" (the existing action) rather
  than "Download" / "Open with…". By then the clip is already on disk.
- **Frame step** has no 150 ms throttle. Each press is one step, and the
  player's step is synchronous.
- **Tooltips carry a `Kbd`** only where a key exists: play (Space),
  fullscreen (Alt+Enter, or Esc to leave). Mute, speed, quality and PiP have
  plain tooltips.
- **A missed Space release.** If the key-up never arrives (the window lost
  focus mid-hold, or a platform dropped it), a press 2.5 s after the last
  sign of Space (`BOOST_STALE_MS`) starts fresh instead of being read as a key
  repeat. A deactivated window also ends a hold.
- **Fade restart.** `fade_controls` restarts its animation at t = 0 on each
  show/hide flip. tdesktop animates from the current opacity, so a flip
  halfway through a fade jumps in Quill instead of turning around smoothly.
- **Layout** follows the design (scrubber row on top, buttons under it).
  tdesktop puts the scrubber at the bottom of the panel.
- **Two demo kinds** were added (`ready-video-controls`,
  `ready-video-speed`). PRs 3 and 4 reuse them for their own captures.

## Supersedes

- `codex-viewer-controls.md`: the "Decision" bullets. Icon buttons, slider
  padding and the zoom controls for photos still apply, but the panel is
  now `VideoController` with the composed scrubber. Space no longer toggles
  on press: it is decided on release, and a hold boosts.
- `codex-viewer-video-extras.md`, "What changed", the quality bullet: the
  quality popover button now lives in the controller's right group with the
  overlay variant. The behavior is unchanged.

## How verified

- Unit tests:
  - `BoostState`: tap, hold, repeat, cancel and a missed release.
  - The waiting delay.
  - `viewer_key_action`: Space and frame step.
  - Scrubber math: fraction, buffered spans, pointer fraction, labels.
  - Speed badge, volume icon tiers, `clear_clip`.
  - WCAG contrast of the labels on the panel.
- GPUI UI tests (`controls/tests.rs`, light and dark): playing and paused,
  muted and level, the boosted "2×" label, the spinner only after the delay
  (test clock), the failed plate.
- An app-level test over a scripted backend (`video_player/tests.rs`,
  `space_taps_toggle_and_holds_boost`) drives the whole window:
  - A Space tap pauses on release.
  - A hold sets rate 2 after 200 ms, does not toggle, and restores the rate
    on release.
  - `.` and `,` step forward and back while paused.
  - Deactivating the window mid-hold restores the rate with no key-up
    (`a_hold_ends_when_the_window_deactivates`).
  - A 0.3 s clip keeps one seek slider across ticks
    (`a_short_clip_keeps_one_seek_slider`).
  - The key probe lets go of focus when the viewer closes
    (`the_key_probe_lets_go_when_the_viewer_closes`).
  - The controller lies inside the window in layered mode
    (`the_controller_sits_inside_the_window`).
- The chat-window key-up path is covered by reading the code only: no test
  drives a second window.
- Captures (`QUILL_DEMO_CAPTURE`, 1200 x 900, both themes):
  `ready-video-controls`, `ready-video-speed` and `ready-video-pip`. Checked
  for overlap, clipping and contrast. The panel, labels, hover thumb, time
  bubble, volume slider, 1.5x pill and popover all render cleanly in both
  themes. In `ready-video-pip` the viewer hands off to the PiP window, so the
  main window shows the chat. The PiP window itself is outside the
  main-window capture (as noted in PR 1).
- `cargo clippy --features demo-capture --all-targets -D warnings`,
  `gate.sh`, file-size and hotspot checks.

Not verified:

- Real buffering on a slow network: the spinner path is covered by the
  test clock and the scripted stall only.
- The download ring against a real TDLib download.
- Live-account playback.
- The "Seek" and "Volume" names with a real screen reader. The patch only
  sets `aria_label` on the slider's root, like any other labeled element.
- Linux and Windows: built by CI only, not run.
