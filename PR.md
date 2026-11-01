Pick a time in a repeated hour just before it first shows, and see suggested post times and the voice chat countdown on the same clock as the pickers

## What was wrong

- N65: the repeated-hour decision doc and the doc comments said `picked_instant` keeps "the instant a picker opened at". That reads as the moment the picker was opened. It keeps the instant the picker was opened showing.
- N66: on the night clocks go back, picking 01:30 at 01:29:58 CDT gave "Pick a time in the future." The resolver took the first 01:30 (CDT, two seconds away) because it was after now, even though the picker's minimum is 10 seconds and the clock shows 01:30 again an hour later (CST). The voice chat, restriction, poll deadline, invite link and suggested post pickers had the same gap, each with its own minimum.
- N67: the suggested post time row (`suggested_posts_boxes.rs`), the Accept Terms text (`suggested_posts_decide.rs`), the suggested post bar (`suggested_posts_ui.rs`) and the "will start in" line of a scheduled voice chat read `now_unix()` and the OS zone. The pickers next to them read `WallClock`. So a label could disagree with its picker, and tests could not pin it.

## What changed

- N65: the doc and three doc comments now say "opened showing".
- N66: `civil_to_unix_with` and `picked_instant` take `earliest`, the first instant the picker accepts, not `now`. `picked_unix` takes each picker's minimum lead: `MIN_SCHEDULE_SECS` (10), `SCHEDULE_MIN_AHEAD_SECS` (12, voice chat), `MIN_RESTRICT_SECS` (30), `POLL_DEADLINE_MIN_SECS` (60), `CUSTOM_DATE_MIN_AHEAD` (10, invite link), the channel's delay plus a minute for suggested posts (new `SuggestLimits::min_lead`, which `time_range` also uses), and 0 for the composer date box and the away schedule, which accept past times. If the first showing is too soon, the second is used. When both are too soon, the result is the first, as before, and the validator rejects it. The decision doc moves this case out of "Out of this slice" and into "What Quill does".
- N67: `time_value` takes a local-time function, and a new `time_label` in `suggested_posts_ui.rs` feeds it `WallClock`. All three labels use `time_label`. The countdown reads `WallClock::of(cx).now()` and is split out as `notified_line` so a test can read it.

## What ran

- `cargo fmt`. `cargo clippy -D warnings` for `--features demo-capture --all-targets` and `--no-default-features --all-targets`: clean.
- New tests:
  - `schedule::tests::a_first_showing_too_soon_to_take_gives_way_to_the_second`
  - `composer_sends_tests::a_time_picked_just_before_its_first_showing_takes_the_second`: the composer schedule picker at 01:29:58 CDT schedules 01:30 CST and shows no error.
  - `suggested_posts_tests::the_accept_terms_date_reads_the_pickers_clock`: shows "Nov 1 at 01:20" on the Chicago test clock.
  - `the_schedule_link_opens_the_schedule_box_on_the_next_hour` now asserts the line "...will start in 1 hour."
- Each UI test was run against the old code and failed: the picker test kept the dialog open with "too soon", the Accept Terms text showed the UTC time, and the countdown said "1 minute" from the real clock.
- Test modules for every changed caller passed: schedule, composer sends, composer bundle, composer date, group call create, group permissions, invite link, poll deadline, suggested posts, business, group moderation.
- `scripts/check-hotspots.sh`: OK. No hotspot file changed.

## What did not run

- `cargo check --target x86_64-pc-windows-gnu`: the target is not installed in this container. The diff has no platform-specific code.
- No macOS run. No macOS-only code changed.
- There is no number to measure in these items.

## What stays open

- `suggested_posts_decide.rs` still reads `now_unix()` for `delete_loses_payment`. That is a check made when the action runs, not a label, and it was not in the bundle.
