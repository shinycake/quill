# Re-review of r3-08 fourth fix round (Opus, 2026-11-12). Verdict: merge

Line numbers are on origin/claude/sweep-r3-08.

The fourth fix round is one commit, fdc39e075, on top of b4702dcd9. It changes three files: src/ui/composer_state.rs, src/ui/composer_sends_tests.rs and docs/decisions/account-scoped-ui-state.md. It touches no baseline or allow-list file. Where the two bullets of reviews/r3-08-round4.md stand:

- **Stage renamed for the guard, list kept beside the stage by hand: resolved.** `DraftClear` no longer has a `messages` field (src/ui/composer_state.rs:13-17). `ClearStage` is now `Unasked` / `Armed(Vec<SentMessage>)` / `Spent(Vec<SentMessage>)` (:27-37), so the requests the stage waits on are inside the stage. check-stage-enums passes because it finds the `RequestId` through `SentMessage` (:42-48), not because of a word-list rename. The rename from `Waiting` to `Armed` is gone along with the bare variant. Here is how each operation works now:
  - `asked` goes to `Armed` only when it has at least one request (:94-100).
  - `answered` does nothing on `Unasked` (:105-125).
  - `ack` clears only from `Armed`, and both `ack` and `fail` go through `ClearStage::spend`. That turns `Armed(rest)` or `Spent(rest)` into `Spent(rest minus message)` (:65-73, :142-152).
  - `is_unasked` is now `matches!(Unasked)` (:136-138), and `is_settled` is `Spent` with an empty list (:155-157).

  `ack` is never called on an `Unasked` entry. The by-id branch needs `made()`, which is false for `Unasked`, and the slot fallback removes the entry without calling `ack` (src/ui/composer_sends.rs:512-536). So sends that never named a request (the menu-command path) still clear their draft the way they did before.
- **Same-slot album and text test: resolved.** `album_then_text_from_one_slot` (src/ui/composer_sends_tests.rs:1064-1086) sets the album clear and then the text clear on the same `forum_slot(1)`, so the text's clear replaces the album's. The answers then arrive for both requests, and request 1 answers twice. This exercises two things:
  - `album_acks_from_one_slot_never_take_the_texts_clear` (:1089-1106): both album acks return `None` and the slot still has its clear. The text's own ack then takes the clear with `sent == "text"`, and the slot is empty afterwards.
  - `album_failures_from_one_slot_never_drop_the_texts_clear` (:1109-1119): both album failures leave the text's clear, and the text's ack then takes it.

  Together they cover the rule that the slot fallback only takes an `Unasked` clear (src/ui/composer_sends.rs:533, :570), for both the take path and the fail path. The two-slot helper now has the name and comment for what it builds (`album_and_text_in_two_slots`, :981-1009). The decision doc records this as row 14-004 (round 4, one stage value) at docs/decisions/account-scoped-ui-state.md:9838.

Checks on the new commit:
- I found no new defects. There are no new bools, unwrap, expect, panic, indexing or RefCell borrows (the file keeps `#![deny(clippy::indexing_slicing)]`), and no timers or tasks.
- Answers are matched only by `RequestId`, and acks by the temporary `MessageId`.
- Every stage can exit:
  - `Armed` goes to `Spent` on an ack or a failure.
  - `Spent` with an empty list is removed by the caller in the same call (src/ui/composer_sends.rs:524-526, :561-563).
  - `Unasked` is taken by its slot.
  - Any entry also goes when its slot's next send replaces it, or with `forget_account`.
- Lists do not grow without bound. `answered` appends only for a request the entry already holds, and never a duplicate (src/ui/composer_state.rs:118-124).
- The only live callers are `expect_draft_clear` (src/ui/composer_sends.rs:640-645) and `send_prep` (src/ui/send_prep.rs:474-479). Both name their requests, so there is no regression where a draft is cleared for the wrong send or never cleared.
- I ran every guard script on an archive of fdc39e075, and they all pass: implicit-state, stage-enums, request-match, owned-timers, refcell-borrows, screen-state, window-scope, window-root, click-through, dialog-answers, drained-notes, lang, submit-settle, and check-file-size.sh. check-bundle-dlopen.py needs a package directory, so it does not apply to a source tree.
- `rustfmt --edition 2024 --check` passes on both changed Rust files.
- I did not run cargo test.

Notes:
- The second run that round 4 asked for was "first fails, then second acks". The new failure test instead fails both album messages. The mixed order runs the same two `None` branches that are already covered, so this is not a defect. A one-line `take_draft_clear(.., MessageId(-2), ..) == None` after a single `fail_draft_clear(-1)` would make it literal.
- `ClearStage::spend` turns `Unasked` into `Spent(vec![])` if it is ever called on one. No caller does this today. `ack` and `fail` could return "settled" directly instead of needing a separate `is_settled` call, but that is optional.
- Still open from round 4, and still bounded: a `Spent` entry can keep waiting on a message that never acks or fails (one deleted while it uploads, or a request TDLib refuses). There is at most one per slot, and it goes when the slot's next send replaces it or with `forget_account`.
