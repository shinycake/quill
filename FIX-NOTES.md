# Fix notes for claude/fix-final-a

Branched from origin/sweep-all at a619b43df. origin/main had no commits that
were missing from sweep-all at the start. The review is review.md on
claude/review-fix-rounds.

## Sections 1 and 2 checked against the current code

All the fix commits that the review listed as missing are now ancestors of
sweep-all: the fix-b13-b18 commits, the fix-b14 commits, 7c4b3c0e1 and
21035932a.

These items were already closed in the code:

- B13 #1: `Losses::adapters` maps `KeepsDying` to `SoftwareOnly`, and
  QUILL-CHANGES.md describes it.
- B13 #2: `battery_signal::refresh` stores its answer with a compare-exchange
  against what it read, so a newer listener answer wins.
- B13 #5: `hardware_probe_at` returns `next_at()`.
- B13 #6: blocking/QUILL-CHANGES.md has an in-Quill count of `blocking-*`
  wakes.
- B13 #8: the `install_once_built` seam has its test.
- B18 D1: the suggested-post block uses `WallClock`.
- B14 B4: file albums group, and an album drags as one, like tdesktop's
  `itemOrItsGroup`.
- B14 N1: `open_photo_editor` refuses while a Send is held.
  `open_photo_editor_on` runs only in the `held: None` arm of a landed
  render, so it is guarded too.
- B14 N4: a dropped photo goes to the tray as a photo unless the tray already
  sends media as files.

## Fixed on this branch

One commit per finding:

- **Three Windows clippy lints** (a08f9ad79). These are `unused_mut` in
  `ensure_private_dir`, `then` with a closure in battery_saving, and
  `type_complexity` in spell_win.
- **frames_of and the eviction test** (b470cfc94). `frames_of` already
  waits on `Engine::wait_idle`, with no deadline. The eviction test slept
  520 ms per clip instead. The test-only `Engine::left_screen` now moves the
  clip's last showing back by `ON_SCREEN`.
- **B14 B2, three names** (e236de124). A group call title, a contact card's
  first and last name, and an invite-link preview title now go through
  `json_name_str`. A test covers all three.
- **B14 B1, deleted account folder** (42516f9ac). The settings writer now
  holds a mark on a folder that is being deleted. The mark lasts from the
  discard until `remove_dir_all` returns. A submit under the folder is
  dropped meanwhile.
- **Quit flush with windows shown** (db617ee10). The flush now runs in the
  quit future, which GPUI polls after it drops every window. On drop,
  Windows hides the window and X11 destroys and flushes it. A test checks
  that the window's view is gone when the flush runs.
- **Session end reopening the main window** (2beae0ff0). The zoom platform
  now wraps the platform's own quit callback and the quit-request answer.
  The quit counts as asked when Windows ends the session (`WM_ENDSESSION`),
  on a macOS terminate, and when the app allows a quit request.
- **One caption per file** (e63ba305b). Each file of a file album draws its
  own caption under it. A test checks the positions.
- **The Linux test failures** (8741f310b, fff247dba). See the next section.

## The Linux test failures

I ran the full `cargo test --features demo-capture --locked --bin quill` on
the base with the frames_of change. 4 tests failed:

- the media viewer cover test, at its first assertion: no frame was drawn
  at all in 2 s
- `an_unread_message_plays_its_effect_once_when_it_shows`
- `the_ticks_of_an_effect_leave_the_slices_alone`
- `a_chat_in_its_own_window_plays_its_effects_there`

`a_sticker_tick_renders_the_layers_alone` passed in that run. It shares
`open_stickers` with the cover test.

**Cause.** Sticker heads and message effects read wall time, while the test
frame clock runs on the executor's simulated time. Stickers in
`open_stickers` and the send_effects demo played by `Instant::now`, so a
tick drew them only if wall time had moved past a frame. The effect pass
stamped an effect's start and judged its end with `Instant::now`, while the
rest of the pass used `cx.background_executor().now()`. The message-effect
wait did the same. The outcome depended on how fast the machine ran.
54040b8c found the same problem for the chat window cover tests.

**Fix** (8741f310b):

- The effect pass uses the frame's `now` throughout, and so does the wait
  in message_effect_play. In the app both clocks read the same time, so the
  app behaves as before.
- `open_stickers` and the send_effects demo put the sticker player on the
  executor's clock.
- With the clocks consistent, an effect ends at its real length. So
  `the_ticks_of_an_effect_leave_the_slices_alone` now measures the second
  after the arrival settles, which is inside the effect.

**Not the cause: power saving.** fff247dba makes power-saving isolation per
thread. Before it, the state was process-wide until the first
`isolate_for_tests` call, which then switched every thread at once. That
commit message names this as the cause of the failures. It is not. A full
run with fff247dba reverted and 8741f310b kept passed: 2626 passed, 0
failed. The change stays because the old switch was still shared state
between parallel tests. Its commit message overstates what it fixed.

**Evidence.** This is one failing full run before the fix (4 failed) and
two passing full runs after it (2626 passed, 0 failed). One of the passing
runs had fff247dba reverted. A clock race does not fail every run, so
three runs are evidence, not proof.

## Not done

- **QuillApp is not kept on a renderer reset** (B1-2 C, Windows only). The
  reopen still builds a new app. The open chat and dialogs are lost, and the
  TDLib clients close and reopen. To keep the entity, it must be able to
  move to a new window. Today `new_with_demo` registers about 18
  subscriptions bound to its window (`subscribe_in`, `observe_in`), and the
  text fields are built with that window. Doing this right means splitting
  those into a step that runs again on the new window. That is a refactor
  of two hotspot files (app_demo.rs, main.rs). This path runs only after a
  Windows device loss, so I could not run it here. I left it rather than
  ship it untested. It is still an open defect.
- **B6.md is still missing** from claude/sweep-briefs. No ref has it. Only
  the owner can push it or confirm that PR 1312's item list is the whole
  bundle. This branch cannot close it. B3 and B5 are missing from the
  bundles folder too.
- **The mic wake nit** was skipped as asked. It is fixed on
  origin/calls-followups.
- **Sections 3 to 5 of the review** were out of scope for this job.

## What ran (Linux)

- `cargo test --features demo-capture --locked --bin quill`: 2626 passed,
  0 failed, 2 ignored.
- `cargo test --features demo-capture --locked --lib`: 7030 passed, 0
  failed, 23 ignored.
- `cargo clippy --locked --features demo-capture --all-targets -- -D
  warnings` and `cargo clippy --locked --no-default-features --all-targets
  -- -D warnings`: clean.
- `cargo fmt --check`: clean.
- scripts/check-file-size.sh: OK.
- scripts/check-hotspots.sh: exit 0. The one new hotspot line, in
  app_demo.rs, carries a `Hotspot-change` trailer.

## What did not run

- No macOS or Windows build. The Windows clippy fixes and the session-end
  change were type-checked on Linux only where cfg allows.
  `cargo check --target x86_64-pc-windows-gnu` fails in gpui-pre's build
  script, because the container has no MinGW toolchain. A Windows clippy run
  must confirm the three lints are gone.
- No real `WM_ENDSESSION` or hung network home folder.
