# Re-review of r3-12 fix round (Opus, 2026-11-12). Verdict: another fix round

Line numbers are on origin/claude/sweep-r3-12.

Status of the items from reviews/r3-12.md:
- Sticker video encode leak: resolved. The worker wraps its output in `OwnedFile` (src/ui/sticker_set_box/video_create.rs:34-39), and the continuation only awaits it (:41). Dropping the stage drops the result, which removes the file.
- 22-005: resolved. A commit that is first in the queue and whose first send is refused is popped back off, and the call returns `Err` (src/connect/sticker_manager.rs:100-119), so `finish_manager` keeps the edits. The test is src/connect/tests/sticker_manager.rs:199-240. See the minor bullet below.
- 23-002: resolved. `State::timer: Option<(Instant, Task<()>)>` (src/ui/userpic_videos.rs:203). `arm_timer` re-arms when no timer is armed or the armed one's due time has passed, on the executor clock (userpic_videos.rs:673-696).
- SetEditing controls while a pick waits: partly done. The footer's "Edit Stickers" (src/ui/sticker_set_box/mod.rs:502) and the add cell (menu.rs:150, fed by mod.rs:439-444) are disabled unless `editing.is_idle()`. One entry point of the same press is still live, see the first bullet.

Remaining defects:

- **The ⋯ menu's "Reorder" still does nothing while a pick is `Waiting`, and nothing tells the user.** `sticker_set_more_button` (src/ui/sticker_set_box/menu.rs:58-64) always offers "Reorder" for an owned set and calls `begin_edit_stickers`. `begin_reordering` accepts only `Idle` (src/ui/sticker_set_box/state.rs:167-171), so the press is dropped while a chosen sticker downloads or decodes. This is the same failure the previous review reported for the footer button. Fix: pass `this.sticker_box.editing.is_idle()` into `sticker_set_more_button` (call at mod.rs:447-454) and add `.disabled(!idle)` to the Reorder `PopupMenuItem`. The kit item supports it; see src/ui/forward_box.rs:420.

- **Minor: taking back a refused first commit also discards an earlier failure the UI has not shown yet.** `let _ = manager.take_failure();` (src/connect/sticker_manager.rs:117) clears whatever `failure` holds. The queue can be empty while `failure` still holds an undrained `CommitFailure` from the previous commit's last step (`note_commit_failure` or the pump's `note_send_failure`). If the box closes again before the next UI drain, that earlier notice is lost, and its `masks` side may differ from this one. Fix: read `let earlier = manager.failure.is_some();` before `advance_manager_commit`, and clear the failure on the `Err` path only when `!earlier`. Or let `note_send_failure` leave an existing failure alone and have the take-back undo only what this call set.

Otherwise the fix round introduces nothing new: no new unwrap/expect/indexing outside tests, no baseline or allow-list lines (the commit touches no scripts), no platform cfg, and the test uses the `SwitchSender` fake. All guards pass on the tip.
