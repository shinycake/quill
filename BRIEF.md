# Sweep fixer brief (cloud)

You fix one bundle of entries from Quill's full state-sweep register. Quill is a Rust/GPUI Telegram desktop client.
This branch (`sweep/fix-briefs`) holds the briefs only; it is never merged. Your prompt names your bundle file under
`bundles/` and your branch name.

Each bundle line is `{"id","file","line","kind","severity","confidence","title","trigger","effect","fields","fix"}`.
`register.jsonl` here is the whole register (1,898 entries) for context. Line numbers refer to commit bc5f50ef2;
the code has moved since.

## Setup

```bash
git fetch origin sweep/fix-briefs claude/project-thread-mnl7ex
git show origin/sweep/fix-briefs:bundles/<bundle>.jsonl > /tmp/bundle.jsonl
git show origin/sweep/fix-briefs:gate.sh > /tmp/gate.sh
git show origin/sweep/fix-briefs:rules.txt > /tmp/rules.txt
git checkout -B <your-branch> origin/claude/project-thread-mnl7ex
```

`claude/project-thread-mnl7ex` is the integration branch. Branch from it, never from main, and never push to it.

Read before any code: `AGENTS.md`, `docs/contributing/state-machines.md`, `docs/contributing/panic-safety.md`,
`docs/contributing/structure.md`, `/tmp/rules.txt`, and sections 1 to 7 of
`docs/decisions/account-scoped-ui-state.md` (OnScreen per account, ChatUi per chat, WindowSlot per window,
ScreenEpoch/spawn_screen, owner keys, the Asker seam in `src/ui/chat_window/dialog_hosts.rs`).

## The work

- Several entries describe the same bug from two shards (same file and mechanism). Fix it once and list both ids.
- For each entry, read the code at that spot first, and check that it still applies. `plausible` entries are
  unverified: if the bug is not real, skip it and say why.
- Fix the root cause. When entries share a shape (a request re-sent every render after a failure; an answer not
  matched to what asked; a confirm box acting on the live selection or the main window instead of what the user
  confirmed; state keyed by bare chat id across accounts), fix it at the shared seam, and add or extend a CI guard
  in `scripts/` when the shape can be checked mechanically.
- New state is one enum (or `Fetch<T>`) per stage with a way out of every stage (answer, error, cancel, owner gone).
  No new stage bools, no `Option` used as a flag, no two fields kept in step by hand.
  `python3 scripts/check-implicit-state.py` must pass; when you touch a struct listed in
  `scripts/implicit-state-baseline.txt`, convert it and drop its line where cheap.
- A failed request lands in an error state the UI draws, with a retry; never a spinner forever, never a re-ask from
  render. Ask from the transition into the loading stage.
- Boxes use the Asker seam: capture the target (account, chat, window, selection) when the box opens.
- Work off the UI thread goes through the existing background executors. Never add a polling task.
- Telegram Desktop (tdesktop) is the behavioural reference. If a fix needs new user-visible behaviour (for example a
  confirmation box), match tdesktop's wording and flow; if you cannot tell what tdesktop does, take the smallest
  safe choice and say so in your report.
- No new `unwrap`/`expect`/`panic!`/unchecked indexing. Keep every file under 1,000 counted lines (split into a
  sibling module). New user-visible strings follow `scripts/check-lang.py`.
- Add a deterministic test per fix where the behaviour can be tested: no sleeps, timeouts or wall clock; use the
  manual clocks and fakes. Test behaviour, not internals.
- Add one "Sweep fixes (<bundle>)" entry to section 8 of `docs/decisions/account-scoped-ui-state.md`.
- For fixes with a visible result, run the app headless with the screenshot demos (`.claude/skills/quill-run`
  skill) when it is practical, and say in your report which fixes you saw on screen. Never commit screenshots.

## Building

You are on Linux. Install what the build needs (see `.claude/skills/quill-run/SKILL.md` and `.github/workflows/ci.yml`
for the package list). `cargo test --no-default-features` and
`cargo clippy --no-default-features --all-targets -- -D warnings` are the core checks;
`bash /tmp/gate.sh` runs everything CI runs (fmt, every scripts/ check, core and UI clippy, core and UI tests) and
prints `GATE OK` or `GATE FAIL`. It takes longer than one Bash call allows, so start it with
`bash /tmp/gate.sh > /tmp/gate.out 2>&1; echo GATE-EXIT >> /tmp/gate.out` as a background command, then wait with a
Monitor until-loop (`until grep -q GATE-EXIT /tmp/gate.out; do sleep 30; done; tail -30 /tmp/gate.out`).
Never end your turn while anything is running or any entry is left: ending your turn ends your session for good.
You have no time limit; finish every entry. A bundle is done only with `GATE OK` in /tmp/gate.out. Builds are slow: build once, then iterate with
`cargo check` on the feature you are in.

## Commit and push in steps

Commit and push after every few fixes, so finished work survives an interruption. Stage files by path (never `git add -A` or `git add .`); never commit `__pycache__` or build output.
`git push -u origin <your-branch>` (retry up to 4 times on a network error, waiting 2, 4, 8, 16 s).
Commit messages end with `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`. If the hotspot check needs it,
put `Hotspot-change: <reason>` on the line directly above `Co-Authored-By:` with no blank line between.
gate.sh runs `cargo fmt`: after the last gate run, `git status`, and commit anything it changed.
Do not open a PR. Do not merge anything.

## Final message

A table of every id: fixed (commit) / duplicate of <id> / skipped (why). Then the guards added, the tests added,
the last gate line, and anything you could not finish.

## What reviews of earlier bundles caught (avoid these)

An Opus reviewer reads every bundle before it merges. In the first bundles about one fix in four was partial:

- A new Loading stage with no exit when the request fails (the box says "Loading" for good). Every new stage needs
  its error arm wired to the request's error path, and the UI must draw it with Retry or Close.
- Two fields kept in step (a `Fetch<()>` beside an `Option<Vec<_>>`, a `*_failed` set beside a stage enum). Put the
  data inside the one enum.
- Fixing one screen and leaving its twin: grep every reader of the field you changed (e.g. both the Appearance page
  and the chat-look box draw the wallpaper list).
- A snapshot or "validated" value recorded when a request is sent instead of when its matching answer arrives.
- Marking something "asked" before the send succeeded.
- A panic guard that ends a loop but still awaits a panicked task (catch_unwind inside the worker instead).
- Behaviour that differs from Telegram Desktop because the register entry assumed it should: check tdesktop's
  source behaviour first, and skip the entry (saying so) when tdesktop does the same.
- An entry wrongly skipped as "already handled": prove it with a test or fix it.

## S4 bundles

S4 entries are minor, and most are `plausible`. Prove each one first (read the code path end to end, check what
tdesktop does); skip it with a one-line reason when it is not real, cannot happen, or tdesktop behaves the same.
Fix the real ones with the smallest change that removes the cause. A test per fix still applies where testable.
The integration branch already carries every S1–S3 fix: re-read the code, many S4 lines moved or were fixed in
passing (say "already fixed by <commit>" and skip).

## IS bundles (implicit state)

IS entries are not user-visible bugs. Each one names a struct whose stages are kept in step by hand: bools, `Option`s used as flags, a side list beside a stage, or a one-shot slot that nothing clears. The goal is that a re-run of the sweep finds nothing, so a fix means the shape is gone, not commented.

- For each entry, read the struct and every reader and writer of the named `fields`. Replace them with one enum (or `Fetch<T>`, or an existing stage type) whose variants carry their data. Every stage needs a way out: the answer, the error, a cancel, or the owner going away. Answers must be matched to the request that asked (`RequestId` in the stage).
- Keep behaviour the same unless the entry shows a real bug. The register's `effect` says what goes wrong. If the conversion fixes a bug, add a test for it. A pure reshape only needs the existing tests to keep passing; add a test where a new stage exit can be driven.
- Skip an entry only when the fields already form one value or are truly independent (two settings, not two stages). Give the reason in one line. "It works today" is not a reason.
- `scripts/implicit-state-baseline.txt` lists grandfathered fields. Drop the line of every field you convert. Also convert any other baseline field in the files your bundle touches when the change is local. The target is an empty baseline.
- Bundles are cut so that no two bundles share a file or a struct. If you need a change in a file outside your bundle's list, make the smallest one and say so in your final message.
- Section 8 of the decision doc gets one "Sweep fixes (is-NN)" entry with the id table.
- Reviews of the first IS bundles caught renames: a `bool` turned into a two-variant enum (`Idle`/`Asking`, `Unsent`/`Sent`) that still sits beside the real stage, is still reset by hand, or still matches its answer by something other than the request. That is not a fix. The new value must replace the old stage outright, carry its data (the `RequestId`, the resolved value), and leave on every exit without a hand reset elsewhere.
- Keep every precondition the old flags encoded: when the old code needed two flags (`bridge_seen && receiving`), the new stage may not be reachable from one event alone.

## Round 2 bundles (r2-NN)

These come from the 2026-11-11 re-sweep of every file, run on the integration branch after all IS bundles merged.
The whole re-sweep register is `register-r2.jsonl` (1,133 entries); line numbers refer to `origin/claude/project-thread-mnl7ex` at f9fd2d57b.
Each r2 bundle holds the S2 and confirmed S3 entries for its files. While you are in a file, also fix any other `register-r2.jsonl`
entry for that same file when the fix is local, and list it in your table.
Do not spawn subagents or run anything in the background except the gate command with its Monitor wait: ending your turn ends your session.

## File-complete rule (round 2, owner's order)

A file you touch must end with **zero findings**. Your bundle lists every `register-r2.jsonl` entry for its files, all
severities (S2 to S4, IS, crash, rename), confirmed and plausible. For each entry: prove it false (read the readers and
writers, say why in one line) or fix it. "Minor", "works today", "touches many sites", "shared pattern", "owned by another bundle", "not local" and "needs a live call" are not reasons. Every bundle owns its files outright: no other bundle will fix them.

When every entry is done, review each file in your bundle again from the top as a fresh sweep reviewer would, with the
criteria in `RESWEEP-BRIEF.md` on branch `sweep/resweep-plan` (implicit stages, renames, answers not matched by
RequestId, detached tasks, timers running while hidden, re-asks from render, crash risks, unconfirmed baseline lines).
Fix anything new you find and add it to your table as `new-NN`. Your final message lists each file with "0 findings left".

Lessons from round 1, where a re-sweep still found 1,133 entries:
- 205 were renames: a bool turned into a two-variant enum that still sits beside the real stage. Fold the data into
  the one stage value; delete the side field.
- Crash risks count: replace `unwrap`/`expect`/indexing with `get`/`let else`, and non-test `RefCell` borrows with
  `try_borrow` or a design that does not need the cell.
- A timer or watcher held as a detached task cannot be cancelled: keep the `Task` in the stage that owns it.
- Remove a field from `scripts/implicit-state-baseline.txt` when you convert it; never add one.

Reviews of the first round-2 bundles still caught: a newest-request-wins rollback that broke when the newest failed first
(test both orders of two in-flight requests), a locally refused send stored as the same Failed value as a server refusal
(keep them apart when the UI treats them differently), a dropped `Result` from a fetch call leaving a box on "Loading" forever,
and a fix that added an `Option<RequestId>` side field beside the value it guards (put the request inside the stage instead).
