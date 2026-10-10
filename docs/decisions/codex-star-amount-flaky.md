# Star subscription balance field names and a flaky emoji test

## What tdesktop does
tdesktop reads the Star balance from the `StarsAmount` pair of whole stars
plus nanostars. TDLib models this as `starAmount star_count:int53
nanostar_count:int32` (schema/td_api.tl:1232).

## What changed
- `parse_star_subscriptions` read `star_amount.amount`, which does not exist,
  so the balance was always 0. It now reads `star_count`. The fixtures in
  `tests_payments.rs` and `state/tests/payments.rs` used the same invented
  names (`amount`, `nanostar_amount`) and hid the bug; they now follow the
  schema. A new test covers the balance and the wrong-name case.
- Audit: `part.get("amount")` in `parse_price_parts` is correct
  (`labeledPricePart label:string amount:int53`). `money()` in
  `service_action.rs` reads `currency`/`amount`, which match
  `messageGiftedPremium`, `messagePremiumGiftCode` and `messageGiftedStars`.
  `premium_hub.rs` and `messageSuggestedPostPaid` already use `star_count`.
- Flaky `emoji_status_choices_resolution_timing_and_confirmed_clear`:
  `prepared_tmp` named its temp dir from pid plus a clock reading. macOS
  clocks tick in microseconds, so parallel tests could share a directory and
  one test's `remove_dir_all` deleted the other's settings files. The name now
  includes a process-wide atomic counter.

## Verified
Gate, plus the test looped 30 times alongside the full suite.
