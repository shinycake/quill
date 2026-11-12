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
4. Two merge checks at hunk level, to catch a fix that a merge lost without a conflict to show for it:
   - My own merges into the follow-up: for each merge commit, every hunk the branch added that the merge result lacks.
   - The 14 cloud rebase merges that came before r2-26's (r2-02, 14, 17, 19, 20, 30, 32, 33, 34, 36, 37, 39, 40, 42): every line the pre-rebase branch added that is missing from the merge and from the same file at dc633d7f3.

## Coverage so far

Read whole, with nothing S3 or worse:

- `src/state/domains/settings/apply.rs`
- `src/state/domains/stickers/error.rs`
- `src/state/domains/groups/error.rs`
- `src/state/domains/groups/apply.rs`
- `src/state/domains/calls/error.rs`
- `src/state/domains/users/apply.rs`
- `src/state/sticker_gif_types.rs` (the GIF panel and search half)
- `src/ui/composer/mod.rs`, `src/ui/composer/bot_thread.rs`
- `src/ui/on_screen.rs`
- the three merges of the last batch as merged: r2-31 (forward stage, chat overlay, ttl picker), r2-41 (payment outcomes, emoji access answers, transport), r2-38 (right press, origin rows, boost hold, loop pause, location search)

Read while tracing a caller, in r2-26's set (not counted): `src/state/session_apply_ok.rs`, `src/ui/chat_ui_state.rs`, `src/ui/esc_stack.rs`, `src/ui/app_render/mod.rs`, `src/ui/app_render/window_upkeep.rs`, `src/state/domains/threads/bot_thread.rs`.

Everything else in scope is unread in this pass.

## Merge checks

- My merges: 17 clean branch hunks in non-test code are missing from the follow-up. Each one is a shape I chose not to take at the merge and recorded in that bundle's note (r2-03b `ListOp<K>`, r2-04 `store_ai_answers`, r2-14 `LookingUp` and `AttachedFound`, r2-22 `age_check.follow`, r2-35 `end_mention_search`, r2-27 `export_refused`, r2-25 `fail_direct_member`, and single lines of r2-11, r2-14, r2-19 and r2-37), or code that moved inside the file. None was lost by accident.
- Cloud rebase merges: 265 files, tests included, with branch lines missing at the tip (r2-20 has the most, 74 files). I went through every non-test file. In each the integration side holds the same fix in another shape: the track list (`ActiveTrack`), the login URL stage, inline query and web app answers by request, the event log wait, `PingStatus::Checking(RequestId)`, `VotersMore::Asking`, quick replies through the request registry, `StorageDrag::Sent`, `Trust::Unreadable`, the bot lookup as three fetches, `AutoDownloadAnswers::take(request)`, the earn and giveaway stages. Two leftovers are listed under S4 below (gift collections, `gifs.search_chat`).
- Not checked: the test files those merges dropped (for example r2-20's `src/state/tests/payments.rs`, 61 lines, and `src/connect/tests/earn.rs`, 31). A dropped test of a shape that was not taken is expected; a dropped test of behaviour both shapes share would be a loss of coverage, not of a fix.

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
- Gift collections (05-031, r2-20): `CollectionsFetch` now holds the request id and only its own answer settles it, but there is still no failed stage: a failed `getGiftCollections` settles to `Idle` and the tabs are missing until the list is reopened. r2-20's `Fetch<Vec<GiftCollection>>` with a Retry button was dropped at its rebase round. Left as it is: Telegram Desktop does the same (`InnerWidget::loadCollections` marks the collections loaded in `.fail` and shows no error), and Telegram Desktop behaviour comes first.
- `gifs.search_chat` is a field beside `GifList::Search` (r2-19's shape that put the chat inside the stage was not taken). One place writes it (the search's start in `src/connect/gifs.rs`) and one reads it (the next page), so it cannot drift; a search that ends leaves the old chat behind, which nothing reads.
- `ViewerUi::swipe_timer` (`SwipeTimer`) sits beside `swipe` (`SwipeNav`). The timer is dropped on close and a late fire checks the swipe before acting, so the two cannot disagree in a way the user sees.
- Transport (`r2-41`): a status that is both at-end and stalled while a loop seek lands stays `Playing`. Accepted at review.

## Not read yet

The rest of the 335 hand-resolved files (next: `src/state/info_types.rs`, `src/connect/core/mod.rs`, `src/state/domains/stickers/apply.rs`, the settings, messages and bots reducers, `src/connect/stickers.rs`, `src/connect/bots.rs`), then the guard-hit files, then the remainder of the 1,353.
