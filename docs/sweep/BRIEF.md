# Exhaustive implicit state sweep: worker brief

Quill is a Rust/GPUI Telegram desktop client. Seventeen sampled review rounds
found 25 to 72 state bugs each, so this sweep reads every non-test source
file once, line by line, and writes down every implicit state machine and
every state bug. Nothing is fixed in this pass; it only finds and records.

You are one of 14 workers. Your shard is `docs/sweep/shards/NN.txt` (NN is
in your prompt): one file path per line. Read **every line of every file**
in it. Coverage is the whole point of this sweep: a file you skim is a file
nobody read.

## Read first

- `AGENTS.md` and `docs/contributing/state-machines.md`: the rule (stages
  are one enum, every stage has a way out, every answer is matched to what
  asked, state lives with its owner).
- `docs/decisions/account-scoped-ui-state.md` sections 1 to 7 (skim section
  8, the per-round history): `OnScreen` is per account and replaced whole
  on an account switch, `ChatUi` is per chat/topic, `WindowSlot` is per
  window, plus `ScreenEpoch`, `spawn_screen`, owner keys (`OfAccount`,
  `ChatMessage`, `AccountChat`, `OwnedBy`) and the Asker seam for boxes.
- `docs/decisions/lock-is-a-render-state.md`.

## What to record

1. **Implicit state machines.** A stage held as bools or Options kept in
   step by hand (`loading`/`loaded`/`error`, `sent` + `pending`, an
   `Option` used as a "started" flag, two fields that must change
   together). Record them even when no bug follows today.
2. **State bugs.** Anything with a concrete trigger that leaves the user in
   a wrong or stuck state:
   - a flag set before a request and cleared only on success (sticks on
     error, on a dropped continuation, on an account switch);
   - an answer not matched to what asked it (by request id, chat, topic,
     account, window, or a token that can repeat);
   - state that outlives its owner (account, chat, topic, window, viewer,
     box) or leaks across owners;
   - main-window-only upkeep for something a chat window can also show;
   - a redraw or request loop (something asked or notified every frame);
   - a box that closes as if saved when the save failed;
   - work that keeps running while hidden, locked or after logout.
3. **Crash risks you happen to see** (`unwrap`, `expect`, unchecked
   indexing, a `RefCell` borrow held across a call that can re-enter):
   record them with kind `crash`.

Confirm every bug by reading the code it depends on, also outside your
shard when needed (grep for writers and readers of the field). Give the
trigger as concrete user steps. If you cannot confirm it, still record it
with `"confidence": "plausible"`.

## Severity

- **S1 critical**: data loss, a message sent/deleted/edited wrongly, money
  (Stars, payments) spent with the wrong result, a privacy leak (secret
  chat capturable, content shown under the passcode lock or in the wrong
  account), a crash, or a UI-thread freeze.
- **S2 high**: a user-visible stuck state that needs a restart or reopen,
  state leaking across accounts/chats/windows, an action applied to the
  wrong target, a redraw or request loop.
- **S3 medium**: a wrong state that corrects itself on the next update or
  reopen, a missing error, a lost toast, wasted work while hidden.
- **S4 low**: cosmetic, or needs an unlikely race.
- **IS**: implicit state machine with no bug found today.

## Output

Write one JSON object per line to `docs/sweep/register/NN.jsonl`:

```json
{"id":"NN-001","file":"src/ui/foo.rs","line":123,"kind":"bug|implicit-state|crash","severity":"S1|S2|S3|S4|IS","confidence":"confirmed|plausible","title":"one line","trigger":"user steps","effect":"what the user sees","fields":["Struct::field", "..."],"fix":"the enum or type that removes it, one or two sentences"}
```

Also write `docs/sweep/register/NN-coverage.txt`: every path from your
shard with its line count and `read` once you have read all of it. Every
path in the shard must appear as `read`. If you could not finish a file,
mark it `partial <last line read>` and say why in your final message.

## How to work

- You may split your shard across subagents (at most 4 at a time), giving
  each a list of files and this brief. Check that their coverage lines add
  up to the whole shard.
- Do not edit any source file, do not build, do not open a PR.
- When done, commit only `docs/sweep/register/NN.jsonl` and
  `docs/sweep/register/NN-coverage.txt` on a new branch `sweep/register-NN`
  (from `origin/sweep/plan`) and `git push -u origin sweep/register-NN`.
  Retry the push up to 4 times on a network error.
- Final message: entry counts by severity and kind, and the coverage line
  (files read / files in shard).
