# Per-slice decision records

`DECISIONS.md` is a historical archive — feature PRs no longer edit it (it was
the #2 merge-conflict source). Slice specs, decisions, and "out of this slice"
future-work notes now live here, one file per slice:

`docs/decisions/<slice-id>.md` — same `<slice-id>` as your
`parity-fragments/<slice-id>.txt` (branch name with `/` → `-`).

Unique filenames mean these can never merge-conflict. Write whatever the old
`DECISIONS.md` per-slice sections held: what you decided, why, what you
deliberately left out, and where the follow-up work lives.
