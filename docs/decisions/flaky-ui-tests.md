# Flaky UI tests

## What tdesktop does

Nothing to match; this is test infrastructure.

## Cause

The dialog tests in `rtmp_stream`, `group_call_create`, `gift_value_ui`,
`ton_wallet_ui` and `ton_wallet_money` measure a button with `debug_bounds`
and then click its centre. gpui-component's dialog entrance
(`Animation::new(..)` in `dialog/dialog.rs`) runs on the wall clock
(`Instant`), which the test executor's `advance_clock` does not move. How far
the dialog had slid when the bounds were read depended on how quickly the test
thread got there, so under CPU load (the full suite runs a few hundred
windows in parallel) the click landed on a different spot than the bounds
said, and the click did nothing. Failures were spread over whichever test
clicked a button right after a dialog opened: `rtmp-start-key`, `rtmp-start-go`,
`group-call-create-schedule`, `group-call-create-confirm`. The same test showed
different `y` bounds on different runs (509 vs 507 for `rtmp-start-go`).

A first attempt, setting `set_reduce_motion(true)` right after `gpui_kit::init`,
did nothing: `QuillApp::new_with_demo` resets the flag from the power-saving
state while it is built (`appearance/mod.rs`).

No other shared state was involved: the statics in the UI layer are
thread-local or isolated per test, and the tests use no sleeps, ports or temp
files.

## Change

Each affected test helper (`frames` / `settle`) sets `cx.set_reduce_motion(true)`
after the app exists, so dialogs are drawn at rest and the bounds are the same
on every run. No retries and no longer timeouts. The animation itself is not
tested here; the kit tests it.

## Verification

Full UI test binary (`--features demo-capture`, filter `ui_tests`, 43+ tests in
parallel) run in a loop on a machine with a load average of 30 to 45 from other
builds:

- Before: 21 of 48 suite runs failed (5/12, 12/20, 3/8, 1/8), always in the
  tests of the two dialog modules above.
- After: 0 of 20 suite runs failed.

`gift_value_ui` and the two wallet modules did not fail in the runs above but
had the same click-after-open pattern, so they take the same fix.
