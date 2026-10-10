# parity-fragments

Feature PRs declare the README checklist items they complete here — one file
per slice — instead of editing `README.md` directly. The merge script checks
the boxes on `main` right after the PR merges, with a direct push (it is the only writer of
`README.md`, so checkbox conflicts are impossible by construction).

## Format

File: `parity-fragments/<slice-id>.txt`, where `<slice-id>` is your branch
name with `/` replaced by `-` (unique per slice, so files never collide).

One `parity:<area>-<slug>` ID per line. `#` starts a comment. Blank lines
ignored.

```text
# loop1 platform spellcheck — genuinely working, adversarially reviewed
parity:platform-spellcheck
parity:platform-spellcheck-languages
```

## Rules (`scripts/check-parity-fragments.sh` checks them locally)

- Every line must be a valid `parity:` ID that exists as a `<!-- parity:… -->`
  anchor in `README.md`'s Status section.
- No ID may appear twice across fragments in the same PR.
- Only declare items that genuinely work — the adversarial reviewer verifies
  each declared ID against the implementation, exactly like checked boxes
  before.
- NEVER edit `README.md` or `DECISIONS.md` in a feature PR. CI fails the PR
  if you do. Slice decisions/spec notes go in `docs/decisions/<slice-id>.md`.

Parity may lag a merge by a few minutes (the pipeline applies fragments
serially after each merge). `scripts/parity_pct.sh` still reads `README.md`
on `main` — no changes needed anywhere downstream.
