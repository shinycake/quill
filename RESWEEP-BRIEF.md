# Implicit-state re-sweep: worker brief

Quill is a Rust/GPUI Telegram desktop client. Two sweeps have read every non-test source file. Round 1 (1,949 findings) and
round 2 (1,133 findings) have been fixed or skipped with a reason, and four CI guards
now block the recurring shapes. This is the confirm pass: it reads every file again and
records every implicit state machine and state bug that is still there. The sweep is
done when this pass finds no confirmed S1, S2 or S3. Nothing is fixed in this pass.

You are one of 24 workers. Your shard is `shards/NN.txt` on this branch (NN is in your
prompt): one file path per line. Read **every line of every file** in it.

## Read first

- `AGENTS.md`, `docs/contributing/state-machines.md`, `docs/contributing/panic-safety.md`.
- `docs/decisions/account-scoped-ui-state.md` sections 1 to 7; skim section 8, which
  lists every "Sweep fixes (...)" entry with the skips and their reasons.
- `scripts/implicit-state-baseline.txt` (grandfathered fields) and
  `scripts/implicit-state-allow.txt` (fields allowed with a reason).
- The four shape guards and their lists: `scripts/check-request-match.py`,
  `check-owned-timers.py`, `check-refcell-borrows.py`, `check-stage-enums.py`, each with a
  `*-baseline.txt` and `*-allow.txt`. Code they already flag is known; still record it if
  it causes a bug (severity S1 to S3), and do not spend time on it otherwise.

## What to record

1. **Implicit state machines.** Stages held as bools or `Option`s kept in step by hand,
   two fields that must change together, an `Option` used as a "started" flag, a
   one-shot slot nothing clears. Record them even when no bug follows today.
2. **Renames.** The last pass turned many bools into enums. Record an enum that is
   still the old flag in disguise:
   - a two-variant enum (`Idle`/`Asking`, `None`/`Running`, `Open`/`Closing`) that
     sits beside the real stage of the same thing;
   - a stage reset by hand from somewhere other than its own transitions;
   - an answer matched by purpose, kind, chat id or a bool instead of the
     `RequestId` held in the stage;
   - a stage that names a timer or watcher while the `Task` is `.detach()`ed
     instead of held in the stage.
3. **State bugs**, with a concrete trigger:
   - a flag set before a request and cleared only on success;
   - a new value written over an in-flight stage, dropping its request or task;
   - `Failed` treated as loaded, so a retry is never sent; or a request re-sent from
     render/upkeep after a failure;
   - a stale/refetch mark lost when the refetch send fails;
   - state that outlives its owner (account, chat, topic, window, box) or leaks
     across owners;
   - a redraw or request loop; work that runs while hidden, locked or logged out.
4. **Skips that do not hold.** If a section 8 skip reason for a field in your shard is
   effort ("touches N sites", "needs a live call", "works today") rather than "one
   value" or "independent", record the field.
5. **Crash risks** you happen to see: `unwrap`, `expect`, unchecked indexing, a
   non-test `RefCell` `borrow`/`borrow_mut`. Kind `crash`.

A field listed in `implicit-state-baseline.txt` is still a finding: record it (kind
`implicit-state`, severity `IS`, title starting "baseline:") so the register shows what
is left.

Confirm each finding by reading its readers and writers, also outside your shard.
If you cannot confirm it, record it with `"confidence": "plausible"`. Do not record
a field whose section 8 skip reason shows it is one value or independent.

## Severity

S1 critical (data loss, money, privacy, crash, freeze); S2 high (stuck state needing
restart/reopen, cross-owner leak, wrong target, loop); S3 medium (self-corrects,
missing error, wasted work); S4 low; IS (implicit state, no bug today).

## Output

One JSON object per line to `register/NN.jsonl`:

```json
{"id":"NN-001","file":"src/ui/foo.rs","line":123,"kind":"bug|implicit-state|rename|crash","severity":"S1|S2|S3|S4|IS","confidence":"confirmed|plausible","title":"one line","trigger":"user steps","effect":"what the user sees","fields":["Struct::field"],"fix":"the enum or type that removes it"}
```

And `register/NN-coverage.txt`: every shard path with its line count and `read`.

## How to work

- Do not spawn subagents or background tasks; read the files yourself, in order.
- Commit and push `register/NN.jsonl` and the coverage file every 25 files or so, so
  progress is never lost. Do not end your turn until every shard file is read and pushed.
- Severity is about what the user sees. Confirmed S1 to S3 needs a concrete trigger you
  traced through the real readers and writers; anything less is `plausible` or S4.
- Do not edit source, build, or open a PR.
- Commit only `register/NN.jsonl` and `register/NN-coverage.txt` on a new branch
  `sweep/resweep3-NN` from this branch, push with up to 4 retries.
- Final message: entry counts by severity and kind, and files read / files in shard.
