# Refactor guardrails: file size and hotspot checks

The last part of the structure refactor. Parts 1 to 5 moved demo, dialog,
state and request registration out of the shared files and split the long
files. Nothing stopped them from growing back, so this adds two CI checks
and a page for contributors.

## What changed

- `scripts/check-file-size.sh` fails when a `.rs` file under `src/` or
  `crates/` passes 1,000 lines. Test-only files and trailing
  `#[cfg(test)] mod` blocks do not count. The 57 files already over the
  limit are in `scripts/file-size-baseline.txt` with their current counts;
  they may shrink, never grow, and must leave the baseline once under the
  limit. `--update` lowers entries and drops the ones under the limit, and
  never adds or raises one.
- `scripts/check-hotspots.sh` reads `scripts/hotspots.txt`: 22 shared files
  (module lists, `main.rs`, `ui/app.rs`, `ui/app_demo.rs`, `ui/shell.rs`,
  `ui/screenshot_demo.rs`, the request and payload hubs, `Session`), each
  with a rule for what a normal PR may change (`none`, `mod-lines`,
  `max:N`, or a regex). It diffs from the merge base with `origin/main`
  (the PR base SHA in CI) and fails past the rule unless the PR has the
  `hotspot-ok` label or a commit carries a `Hotspot-change:` trailer. It
  also warns when a changed file gains a `match` with more than 30 arms or
  a `pub enum` with more than 40 variants.
- Both run in the `linux-fmt-clippy-test` job, before fmt. The hotspot step
  runs on pull requests only.
- `docs/contributing/structure.md`: where a feature, demo, dialog, request,
  update handler and app state go, the size limit, the hotspot list and
  the screenshot policy.
- `Cargo.toml`: the `ui` feature lists one dependency per line, so two PRs
  adding a dependency no longer edit the same line.
- `tests/structure_checks.rs`: 10 fixture tests. The file-size ones run on
  `tests/fixtures/file-size/` with a limit of 10; the hotspot ones build a
  throwaway git repository per test.

## Choices

- Both scripts are bash and awk (the event JSON is read with `python3`,
  which the CI image has), and they run with macOS's bash 3.2, so they work
  locally without extra tools.
- The baseline only has to allow a file to stay the same or shrink. Making
  every shrink update the baseline would turn the baseline itself into a
  file every PR edits.
- A GitHub label is read from the event payload when the job starts, so
  adding `hotspot-ok` after the run needs a re-run of the job. Triggering
  CI on `labeled` would also re-run the three packaging jobs.
- The match and enum counts follow rustfmt layout (arms one indent level
  inside the `match`). They are warnings because a big exhaustive match can
  be the right shape.
- This PR does not edit `.claude/agents/*.md`. Those files configure the
  agents, so the owner adds the pointer line to
  `docs/contributing/structure.md` there.

## Verification

Gate (fmt, core and UI clippy, core and UI tests): core 3029 (3019 plus the 10
new tests), UI 234. On this tree `check-file-size.sh` passes with the
baseline, and `check-hotspots.sh` against the 3c branch passes.
