# Re-review of r3-06 fix round (Opus, 2026-11-12). Verdict: merge

Line numbers are on origin/claude/sweep-r3-06.

Fix-round commit checked: 4eb8ca7c4. Where each earlier bullet stands:

- One-line `generation` variants slipped past the guard: resolved. `generation_hits` now also tests the `VARIANT` line itself against `GENERATION_INLINE` (scripts/check-request-match.py:66-79) and keeps the multi-line check. `self_test()` runs before every check (:104-113): it feeds in the old `DeepLinkInfo { generation: u64 },` and expects a hit, covers a multi-line variant, and checks a negative case.
- Phone-share answers with no owner left in the queue: resolved. `WebApps::take_phone_share` drains the whole `Vec` with `mem::take`, returns only the answer for the request being waited on, and drops the rest (src/state/web_app_types.rs:130-141). `poll_phone` calls it on every mini-app poll pass. It passes the window's `PhoneFlow::Sending` request, or `None` when the window is gone or not sending, and it drains before checking for a window (src/ui/web_app_ui/phone.rs:204-217). Covered by src/connect/tests/web_apps.rs:683.

No new defects found. The fix adds no stage flags, unwrap, indexing, borrows, static counters or allow-list entries. The new test is deterministic. All scripts/check-*.py guards (check-bundle-dlopen.py needs an argument and is not a guard), check-file-size.sh and test_shape_guard.py pass at the tip.

Notes: the mini-app poll stops once no window or flow is left (src/ui/web_app_ui/mini_app_poll.rs:83-88). An answer that lands after the window closed therefore stays in `phone_share` until the next mini app opens and its first poll pass drains it. That is at most one entry, so it is bounded and not a leak. This branch and r3-03 both change scripts/check-request-match.py: r3-06 adds the `generation` rule and r3-03 adds the `ticket` rule, and both rules hit `generation: u64` purpose variants. Expect a merge conflict there, and dedupe the overlapping rules when merging.
