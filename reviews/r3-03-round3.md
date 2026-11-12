# Re-review of r3-03 second fix round (Opus, 2026-11-12). Verdict: merge

Line numbers are on origin/claude/sweep-r3-03.

Commit checked: d907ce860. Where the round-2 bullet stands:

- The poll asked for the typed text instead of the stale list: resolved. The poll now calls `refresh_shown_member_dialog` (`src/ui/notifications/poll_drains.rs:203`), which builds the request from `dialog.searched` (`src/ui/group_members.rs:107-113`). It shares `refresh_member_dialog_list` (`:115`) with the Search button and tab paths, and those still read the text box. Its filter choice matches `member_dialog_filter` (`:662`): All with an empty `searched` gives Recent, All with a query gives Search, any other tab gives its filter through `shown_for`. So the poll refreshes exactly the key it found `Stale`. The refused-send loop is closed too. A refused refresh of a `Stale` list goes to `note_member_refused(key, query)` (`src/connect/moderation/members.rs:111`) under the same key, which becomes `NotSent`. `member_dialog_is_stale` is then false, and the poll does not repeat the send, the note or the redraw. The no-right and not-a-group cases drop the stale list (`drop_stale_member_list`).
- Notes from round 2: the Retry test goes through the UI Retry again (`src/ui/inline_members_retry_tests.rs:140`). The new test is `the_poll_refreshes_the_shown_list_not_the_typed_text` (`src/ui/inline_members_retry_tests.rs:265`).

New-defect pass on d907ce860: no new flags, no new matching outside RequestId, no new non-test unwrap/expect/indexing, and no new baseline or allow-list entries. Every `scripts/check-*.py` guard passes on the tip, along with `check-file-size.sh` and `test_shape_guard.py`. (`check-bundle-dlopen.py` needs a bundle argument and is not a source guard.)

None.

Notes: the new test calls `refresh_shown_member_dialog` directly on the demo window, which has no live driver. It shows that `searched` does not take the typed text, but it never runs `members_changed` or the poll, and it does not check that no `Search` request goes out. A connect-level test would cover more: a `Stale` Recent list, text in the box, one poll pass, one Recent request out. `a_viewer_without_the_right_refreshes_the_list_the_tab_shows` (`src/state/tests/sweep_r3_03.rs:215`) still tests only the pure `shown_for` mapping. The decision doc lists "the poll not searching for unsent text" among the `state/tests/sweep_r3_03.rs` / `connect/tests/sweep_r3_03.rs` tests, but it lives in `src/ui/inline_members_retry_tests.rs`. Merge conflict to expect: this branch and r3-06 both edit `scripts/check-request-match.py` (`ticket` and `generation` rules overlap on `u64` purpose variants). Dedupe them when merging.
