# Kit dialog animations run on the executor clock

Test-only fix; tdesktop is not involved.

`ui::gift_delete_demo::tests::confirming_removes_the_gift_from_the_grid_and_the_box`
failed under full-suite load. The kit alert's entrance is a GPUI
`with_animation`, which took its start and elapsed time from
`Instant::now()`. The test advanced the executor's simulated clock by 600 ms,
which that animation never saw, so whether the alert was ready for the Enter
key depended on real elapsed time. `bars_tests::answering_yes_drops_the_edit`
had the same cause.

## What changed

- `third_party/gpui-pre/src/elements/animation.rs`: `with_animation` and the
  spring element read `cx.background_executor().now()`. In a running app that
  is the wall clock; under `TestAppContext` it is the simulated clock, so
  `advance_clock` drives every animation deterministically. Logged in
  `third_party/gpui-pre/QUILL-CHANGES.md`.
- Tests that waited for a kit dialog or a viewer slide with
  `std::thread::sleep` now call `executor().advance_clock` instead
  (`group_thumbs_ui_tests`, the three passkey test modules,
  `settings_window/tests`, `advanced_ui_tests`). With the fix the sleeps would
  have stopped working. The sleeps for frame-clock animations (top people
  strip, pull to next chat, send fly) stay, since those use their own timers.

## Verification

The full `ui::` suite ran 30 times in a loop with default test threads, before
and after (counts in the PR).
