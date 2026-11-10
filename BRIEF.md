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
prints `GATE OK` or `GATE FAIL`. Run it in the foreground. Builds are slow: build once, then iterate with
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
