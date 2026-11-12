# Re-review of r3-08 second fix round (Opus, 2026-11-12). Verdict: another fix round

Line numbers are on origin/claude/sweep-r3-08.

Second fix round: 0ebef43fd. Status of the bullets in reviews/r3-08-round2.md:

- 15-003 (a lost disk refusal): resolved. `Prepare::DiskRefused { workers }` (src/ui/background_accounts/prepare.rs:20). `refuse` turns `Cancelled { workers }` into `DiskRefused { workers }` (:82-94). `finished` counts the workers down and keeps the refusal at 0 (:150-160). `running` and `keys` stay true while workers > 0 (:60-77). `take_refused` and `cancel` hand any workers still running back as `Cancelled` (:104-132). The retry pass skips only `wanted` (mod.rs:359), so a `Cancelled { n }` left by `take_refused` is asked again.
- 16-006 (`ready: Vec<WithdrawReady>`): resolved in the shape the review suggested. `events: Vec<GramsEvent>` with `GramsEvent::PasswordReady` (src/premium_hub/ton.rs:567-576), drained by `take_events` (:589-591, src/ui/premium_ui/grams_withdraw.rs:171).
- 14-004 for albums: half done. The session side is fixed: `record_send_answer` (src/state/domains/messages/apply.rs:9-23) is called for every album message (src/state/session_apply_messages.rs:32), with a test at src/state/tests/drafts_everywhere.rs:695. The UI side still treats one request as one message, so the clear is still found by slot for every album message after the first. See the bullet below.

Remaining:

- An album's second and later messages still fall back to the slot match (src/ui/composer_state.rs:56-62, src/ui/composer_sends.rs:503-521). `DraftClear::answered` turns `Asked(request)` into `Made(first id)` on the first answer. Later answers for the same request find no `Asked` entry and are dropped. Even with every id kept, the first ack removes the whole clear (`take_draft_clear`), so the other messages' acks find no `made` match. They fall back to `ack_slot`, and there they take whatever clear that slot holds. Because album items upload and ack seconds apart, that clear is often another send's. Two cases:
  - A text sent from the same main composer while the photos upload: an album item's success ack clears that text's draft early, and its failure ack (`for ack in failed`, src/ui/notifications/spawn_call_notification.rs:433-439) deletes the text's clear, so the text's own ack later clears nothing.
  - For a reply from a supergroup's main composer: the ack names the replied-to topic, so the item takes that topic's pending clear.

  In both cases the clear is matched by slot instead of by the request that made the message. Fix: record every temporary id of a request (`SentMessage::Made { request, message }`; `answered` replaces the `Asked` entry and appends a `Made` for each later answer of the same request). Keep the entry after the draft is cleared, in a spent stage (for example `DraftClear` gets `Waiting | Cleared`), until the last of its messages has acked or failed. A later ack of the same send then matches it by id, and `take_draft_clear` never reaches the slot fallback. Add a `composer_sends_tests` case: a two-message album and a text send pending in the same slot, with the album's second ack (success, and in another case failure) arriving after the first. The text's clear must survive both.

The commit adds no unwrap, expect, indexing, RefCell borrows, allow-list lines or baseline lines. The guard scripts pass on the tip. check-bundle-dlopen.py needs a package directory, so it does not apply to a source tree.

Notes: the new `d` case in `a_refused_start_waits_without_blocking_deletion_and_leaves_on_a_retry` (prepare.rs:229-243) calls `refuse` on a `Cancelled` entry whose `finished` returned false, which the app never does. The real path is begin, cancel, begin, then the first `finished` returns true while one worker still runs, then `refuse`. Both reach the same branch, but the test would read better following the real order. `GramsEvent` has one variant and is read with `.contains(...)` into a bool. It is the shape the round-2 review asked for, but folding `WithdrawOutcome` in as a second variant would make it a real event queue.
