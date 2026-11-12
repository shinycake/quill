# Re-review of r3-12 second fix round (Opus, 2026-11-12). Verdict: merge

Line numbers are on origin/claude/sweep-r3-12.

Second fix round: 3518934f3. Status of the bullets in reviews/r3-12-round2.md:

- The ⋯ menu's "Reorder" while a pick is `Waiting`: resolved. `sticker_set_more_button` takes `idle: bool` (src/ui/sticker_set_box/menu.rs:18), and the Reorder item is `.disabled(!idle)` (:60). The caller passes `this.sticker_box.editing.is_idle()` (src/ui/sticker_set_box/mod.rs:453). All three entry points now share one rule: the footer (:503), the add cell (:442) and the menu.
- Taking back a refused first commit no longer discards an earlier unshown failure: resolved. `commit_sticker_manager` sets the earlier failure aside before the first send, clears only the failure this call set on the `Err` path, and then restores the earlier one (src/connect/sticker_manager.rs:114-127). On success, `advance_manager_commit` sets no failure (it sets one only on `Err`), so restoring the earlier value loses nothing. Test: src/connect/tests/sticker_manager.rs:245 uses the `SwitchSender` fake and no clock.

The commit adds no unwrap, expect or indexing outside tests, and no RefCell borrows or new state. It touches no scripts or baselines. The guard scripts pass on the tip, rustfmt reports no changes in the touched files, and check-bundle-dlopen.py needs a package directory, so it does not apply to a source tree.

Notes: none.
