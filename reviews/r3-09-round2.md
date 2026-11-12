# Re-review of r3-09 fix round (Opus, 2026-11-12). Verdict: merge

Line numbers are on origin/claude/sweep-r3-09.

Fix round: 225e286ba. Every bullet of reviews/r3-09.md is resolved:

- 17-002: `Entry::Probing { _job: Task<()> }` owns one foreground task that awaits the background read and replaces the entry with `Known` or `Unreadable` (src/ui/hdr_image/probe_cache.rs:22-30, 72-95). `PROBE_TIMEOUT` and the time-based re-probe are gone (probe_cache.rs:43-50). A full cache keeps running probes (probe_cache.rs:67-71), so a path has at most one probe. The baseline line stays removed, which is correct because the variant now holds the task. The stage-enums guard passes.
- 16-009: `RefusedFiles` is settled against the shown set each upkeep and reset when the last box closes (src/ui/gift_market_state.rs:212-242, 386-388). Refused files are not re-sent (src/ui/gift_market_ui.rs:303-323). The market lives in `OnScreen`, per account.
- 17-006: `unopenable` now holds `Unopenable { path, windows }`, and the same `retire` rules as slots drop it (src/ui/inline_players/windows.rs:63-68, 201-236, 251-253). A row that keeps rendering refreshes its pass (src/ui/inline_players.rs:890-896). The fixer's reason for not using a Slot state holds: there is no player to hold. It shares the slot's lifetime, which was the point of the bullet.
- 17-001: `PackChanges::settle` returns `Out` to `Idle` once its request leaves the pending set (src/ui/group_edit_box/extras.rs:45-56). It runs from the poll's `settle_group_edit_box` (src/ui/group_edit_box/mod.rs:361-367).
- Integration with r3-03: `MembersFetchView` matches every `SupergroupMembersFetch` variant with no wildcard (src/ui/inline_members_view.rs:199-220), so r3-03's `Stale { last }` becomes a compile error at merge. That is the intent.

No new defects. All CI guard scripts pass on the tip, and the fix round adds no unwrap, panic, indexing or wall-clock waits.

- none
