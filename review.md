CHANGES

# Review of PR 1314 (claude/sweep-b18), bundle B18: N65, N66, N67

Reviewed at bb65a25 against main 5a173f5 (merge base 948ad901e).

## What ran

- `cargo fmt --check`: clean.
- `cargo clippy --features demo-capture --all-targets -- -D warnings`: clean.
- `cargo clippy --no-default-features --all-targets -- -D warnings`: clean.
- The touched tests: `schedule::tests` (20), `suggested_posts::tests::date_labels`,
  `a_time_picked_just_before_its_first_showing_takes_the_second`,
  `the_accept_terms_date_reads_the_pickers_clock`,
  `the_schedule_link_opens_the_schedule_box_on_the_next_hour`,
  `the_custom_end_opens_a_day_after_the_installed_clock`,
  `the_date_box_turns_the_selection_into_a_date`,
  `a_schedule_in_the_repeated_hour_confirmed_unchanged_keeps_its_pass`: all pass.
- Fail-without-fix check: with the composer confirm passing lead 0 and
  `time_label` reading `now_unix`/`civil_local`, the two new UI tests fail
  (composer_sends_tests.rs:514, suggested_posts_tests.rs:242).
- Trial merge of origin/main (25 commits ahead of the merge base): no
  conflicts. `scripts/check-hotspots.sh`: OK. Trailers match rules.md.
- Not run: the full test suite, the Windows check, macOS.

## Items

### N65: decision doc says the instant a picker "opened at". Fixed.

`docs/decisions/claude-sweep-repeated-hour.md` now says "opened showing",
and the same wording is fixed in `schedule::picked_instant`,
`WallClock::resolve` and `TimePicker`. The "Out of this slice" section is
removed because N66 closes it. No remaining "opened at" in src or docs
refers to a picker.

### N66: a time picked in the minute before its first showing reads "too soon". Fixed at the root.

`civil_to_unix_with` now takes `earliest` (now plus the picker's minimum
lead) instead of `now`, and picks the first showing `>= earliest`. Every
`picked_unix` caller passes the lead its own validator uses. I checked that
each boundary agrees: every validator rejects `< now + lead`, so `>=`
matches.

| Picker | Lead passed | Validator |
| --- | --- | --- |
| Scheduled message | `MIN_SCHEDULE_SECS` (10) | `validate_send_date` |
| Invite link expiry | `CUSTOM_DATE_MIN_AHEAD` (10) | `custom_date_ok` |
| Voice chat | `SCHEDULE_MIN_AHEAD_SECS` (12) | `check_schedule` |
| Restriction | `MIN_RESTRICT_SECS` (30) | `validate_restrict_until` |
| Poll deadline | `POLL_DEADLINE_MIN_SECS` (60) | `deadline_choice` |
| Suggested post | `limits.min_lead()` | `time_range` / `clamp_time` |
| Composer date box, away schedule | 0 | accept past times |

The signature change forces every caller to choose, so none is left on the
old behavior. The unchanged-picker path still keeps the pass it opened on.

### N67: countdown and suggested post labels read `now_unix()`. Partly fixed.

The three listed labels (`suggested_posts_boxes.rs` time row,
`suggested_posts_decide.rs` accept text, `suggested_posts_ui.rs` bar) and
the voice chat countdown (`notified_line`) now read `WallClock` for both
now and zone. `time_value` takes the zone as a function. Correct. One
label of the same kind is left; see D1.

## Defects

### D1. A fourth suggested post time label still reads the OS clock and zone (N67 residue)

`src/ui/history/suggested_post_block.rs:50-53` builds the time row of the
suggested post block in chat history from
`block_time_label(&civil_local(send_date), &civil_local(now_unix()))`. It
shows the same post time that the PR now reads from `WallClock` in the bar
and the boxes. Under an installed clock, and in any test, the history block
and the bar can name different times for one post. Removing that split is
the point of N67. The bundle named three sites, but the item is "suggested
post label reads `now_unix()`", and this is one of them.

Fix: in `suggested_post_block`, take `let clock = WallClock::of(cx);` and
call `block_time_label(&clock.civil(i64::from(info.send_date)), &clock.civil(clock.now()))`.
Then drop the `civil_local`/`now_unix` imports if nothing else uses them.
Add a test that installs `WallClock::chicago_repeated_hour()` and asserts
the block row reads 01:20 for `CHICAGO_FALL_BACK + 1200`.

### D2. `time_label` was inserted between `price_amount` and its doc comment

`src/ui/suggested_posts_ui.rs:61-68`: the new function sits under
`/// The amount as tdesktop draws it: the currency icon, then the number.`
Rustdoc joins both doc lines onto `time_label`, and `price_amount` loses
its doc. Clippy does not catch this.

Fix: move `time_label`, with its own doc line, above the "The amount as
tdesktop draws it" comment, so that comment sits directly on
`price_amount` again.

## Not defects

- Each call site repeats the lead its validator uses (see the table). They
  agree today. A shared per-picker constant would be tidier, but no wrong
  value ships.
- `suggested_posts_decide.rs:350` still calls `now_unix()` for
  `delete_loses_payment`. That decides an action. It is not a label shown
  on the pickers' clock, so it is outside N67.
- `resolve` and the validators each read `WallClock::now()` separately. On
  the OS clock a second can tick between the two reads. That only matters
  within one second of the boundary, as on main, so it is not new.
- The branch has not merged the 25 newer main commits. It merges cleanly,
  but rules.md asks for a merge before the last push. Merge main before
  landing.
