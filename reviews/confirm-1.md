# Confirm pass 1 (files r2-26 does not touch)

Status: in progress. No S3-or-worse finding so far.

## Scope

- Base: `git diff --name-only origin/main...dc633d7f3` (the follow-up with 46 of 47 branches in): 1,701 files.
- Left out: the files named in `bundles/r2-26.jsonl` and the files in r2-26's own diff (115 files). They are read with r2-26.
- In scope: 1,608 files, of which 1,586 are `src/**/*.rs` and 1,353 of those are not test files.

## Method

1. The four shape guards, `check-implicit-state.py` and `check-screen-state.py` run on dc633d7f3 with `--list`, so every hit has a file and a line (inventory below).
2. Fresh reads, whole file, against BRIEF.md: one enum per staged thing, an exit from every stage (answer, error, cancel, owner gone), answers matched to the request that asked, a failed request drawn with a retry, state with its owner, no two fields kept in step.
3. Reading order: the files I resolved by hand in the 50 integration merges first (335 non-test files, most-merged first), because that code was written by no single author and no bundle review saw it in its final shape. Then the files with guard hits.

## Coverage so far

Read whole, with nothing S3 or worse:

- `src/state/domains/settings/apply.rs`
- `src/state/domains/stickers/error.rs`
- `src/state/domains/groups/error.rs`
- `src/state/domains/calls/error.rs`
- `src/state/domains/users/apply.rs`
- `src/state/sticker_gif_types.rs` (the GIF panel and search half)
- `src/ui/composer/mod.rs`, `src/ui/composer/bot_thread.rs`
- `src/ui/on_screen.rs`
- the three merges of the last batch as merged: r2-31 (forward stage, chat overlay, ttl picker), r2-41 (payment outcomes, emoji access answers, transport), r2-38 (right press, origin rows, boost hold, loop pause, location search)

Read while tracing a caller, in r2-26's set (not counted): `src/state/session_apply_ok.rs`, `src/ui/chat_ui_state.rs`, `src/ui/esc_stack.rs`, `src/ui/app_render/mod.rs`, `src/ui/app_render/window_upkeep.rs`, `src/state/domains/threads/bot_thread.rs`.

Everything else in scope is unread in this pass.

## Mechanical inventory at dc633d7f3

| Guard | Baseline hits | Allowed |
| --- | --- | --- |
| request-match | 144 (122 `purpose`, 28 `unmatched` counting the allowed) | 6 |
| owned-timers | 46 | 9 |
| refcell-borrows | 92 | 0 |
| stage-enums | 183 | 45 |
| implicit-state | 9 | 14 |

333 files carry at least one hit; 97 of them are outside the follow-up's diff. The largest groups are `src/connect/stickers.rs` (20) and `src/connect/emoji_sets.rs` (12): account-wide "one request per purpose" guards, which dedupe a fetch and do not mis-route an answer. They are S4 debt, not findings.

## Findings

### S3 or worse

None so far.

### S4, fixed (waits for the next gate run)

- `PaymentsState::outcomes` grew by one entry per paid invoice for the whole session, and the verdicts of one account's mini app were still there for the next account on screen. Fixed in 9299dd787 (local, on top of dc633d7f3): the list keeps the newest 16, `forget_screen_outcomes` clears it, and a test covers both. It goes onto the follow-up with the r2-26 merge's gate.

### S4, listed

- `WebApps::emoji_access`: an answer nobody waits for (the bot permissions switch asked, the mini app window is closed) stays in the list until the next mini app drains it or the account leaves the screen. Bounded by the number of toggles.
- `src/state/session_apply_ok.rs`: `DeleteAccount` ok sets `account_delete = Fetch::Idle` without checking the request. One request at a time, and the account closes right after.
- `src/state/domains/calls/error.rs`: the shared arm for the video chat's small requests (mute, hand, title, recording, invite) sets `group_call_error` without checking that the failed request's `group_call_id` is the call on screen. A request of a call you left that fails late shows its line on the next call's panel. The screen-share start and stop arms set the line the same way, though they check the id before touching the share stage.
- `src/state/domains/groups/error.rs`: a failed `LoadChatWelcomeMessages` fails the chat's pack through `entry().or_default().fail(..)` with no request match; a failed `ToggleBroadcastGroup` drops the optimistic flag the same way. `member_action_error` is one line per chat that the next change overwrites.
- `src/state/sticker_gif_types.rs`: `GifSearch` holds `fetch: Fetch<()>` beside `results`, `next_offset` and `asked_offset`, and `GifPanel::saved` is a `Fetch<()>` beside `animations`. The pages are matched by request; the saved list is not (`saved_answered` takes any answer). The r2-30 reshaping of this was not taken at merge.
- `src/ui/composer/bot_thread.rs` (`advance_bot_thread`): the field is emptied before `select_topic_ui`. If the topic cannot be opened (the driver refuses: signed out, the chat closed under it) the text then goes to `submit_composer`, which asks for another thread; if that ask cannot be sent either, the text is gone. Needs two refusals in a row on a path that only fails while the account is going away. Not part of the sweep's changes.
- Transport (`r2-41`): a status that is both at-end and stalled while a loop seek lands stays `Playing`. Accepted at review.

## Not read yet

The rest of the 335 hand-resolved files (next: `src/state/info_types.rs`, `src/state/domains/groups/apply.rs`, `src/connect/core/mod.rs`, `src/state/domains/stickers/apply.rs`, the settings, messages and bots reducers, `src/connect/stickers.rs`, `src/connect/bots.rs`), then the guard-hit files, then the remainder of the 1,353.
