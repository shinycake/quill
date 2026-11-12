# Re-review of r3-07 fix round (Opus, 2026-11-12). Verdict: merge

Line numbers are on origin/claude/sweep-r3-07.

Fix round: 48560d61f, f0fd44aeb. Every bullet of reviews/r3-07.md is resolved:

- 12-001: the Shown/Hidden bit is gone. ExportView is deleted from src/export/handle.rs, and the box is one value, `ExportBox::{Closed, Progress { seen }, Options(draft)}` (src/ui/export_state.rs:11-20, 49-71). The poll reads "hidden" as `!is_progress()` (src/ui/export_progress.rs:82, src/ui/account_export_progress.rs:39), and the dialog registry opens on `is_progress() && export exists` (src/ui/export_box.rs:621-625, src/ui/account_export_box.rs:382-386). The value lives in `OnScreen::dialogs`, per account.
- Night clock after sleep: `cx.on_system_wake` re-arms through `apply_appearance` (src/ui/app_demo.rs:318-325 (on_system_wake at 322)). Window activation re-applies while auto-night is Scheduled (src/ui/app_demo.rs:270-281). `sync_night_clock` re-arms on every apply (src/ui/appearance/night_clock.rs:27-53), and an unchanged apply returns early (src/ui/appearance/mod.rs:166-168).
- Timer allow list: the stress loop is now `QuillApp::spawn_stress_redraw` (src/ui/app_demo.rs:335-347), and only that name is allowed (scripts/owned-timers-allow.txt:12-13).
- 14-005: `PendingBan.request` holds the ask (src/ui/communities/mod.rs:167-171). `ban_start` keeps the RequestId (src/ui/communities/ban.rs:60-70). A newer ask for the same member re-points the wait (ban.rs:83-96). The waits end in the close hooks through `drop_bans_over` (ban.rs:153-159, src/ui/shell.rs:514, 564).
- 14-001: `Session::settle_lost_request` settles acks (src/state/requests.rs:186, src/state/acks.rs:41-47), tested in src/state/tests/lost_requests.rs:440-454.
- Request-match baseline: check-request-match passes with no stale lines (146 hits under 121 keys).

No new defects. The CI guard scripts (implicit-state, stage-enums, request-match, owned-timers, refcell, screen/window scope, lang, file size and the rest) all pass on the tip. I found no new unwrap, panic or indexing in non-test code, and no wall-clock tests.

- none
