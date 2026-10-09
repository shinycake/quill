# Rate limits and stale pending requests (Q1, R5)

## What tdesktop does

`lng_flood_error` ("Too many tries. Please try again later.") is the line shown
when a user action hits a flood limit; the MTP layer waits out `FLOOD_WAIT_N`
for background requests instead of failing them. Nothing blocks on a response
that never comes: requests are owned by the session and fail with the
connection.

## What Quill did before

- A 429 ("Too Many Requests: retry after N") was only understood in the login
  flow (`FLOOD_WAIT_N`). Everywhere else it fell through as a raw error.
- `PendingRequest` had no timestamp and was cleared only on account/auth
  invalidation. A lost answer kept the per-chat dedupe guards
  (`ids_for_chat`, `has_purpose_for_chat`) closed and the history skeleton
  spinning forever.

## What changed

Q1, rate limits:

- `parse_error` reads the retry-after seconds from both `FLOOD_WAIT_N` and
  `Too Many Requests: retry after N` (`parse_flood_wait_secs`);
  `TdError::flood_notice()` renders "Too many attempts. Try again in N
  seconds."
- Idempotent reads are re-sent after the wait. The sender (`RetryStash`,
  `src/connect/sender.rs`) keeps the JSON of a fixed list of read `@type`s
  (history, search, getChat/getFile/getMessage..., member/full-info lookups).
  `ConnectDriver::absorb_flood` swallows the 429 for those, keeps the pending
  entry (so dedupe guards stay closed and no duplicate request can start) and
  schedules a re-send with the same `@extra`. Cap: 3 attempts, waits up to
  60 s; beyond that the error reaches the reducer as before. Auth, passwords
  and all mutations are never stashed.
- `request_tick` (called from the existing UI poll loop, no new timer) sends
  the due retries.
- User actions (anything not a lookup/auth/view ping) show the flood notice in
  the status note (`Session::flood_notice`); composer text is untouched because
  nothing clears it on error. A failed send (`updateMessageSendFailed`) also
  raises the notice. The failed row keeps its retry affordance: `can_retry`
  comes straight from TDLib's `messageSendingStateFailed`, which marks rate
  limits retryable (covered by a test that feeds a 429 failed send).

R5, pending TTL:

- `PendingRequest::created_at` is stamped at registration.
  `RequestRegistry::sweep_stale` drops entries older than 60 s whose purpose is
  in `RequestPurpose::is_sweepable` (history pages, searches, shared media,
  pinned list, per-entity info lookups). Downloads, uploads, calls, auth and
  all mutations are never swept. The sweep runs every 15 s from `request_tick`,
  skips requests waiting for a flood retry, and does nothing while the
  connection is not Ready/Updating (TDLib queues requests offline).
- `HistoryState::load_failed` is set when the first history page errors or is
  swept. The empty conversation then shows "Couldn't load messages · Retry"
  instead of the skeleton; `fetch_history` stays quiet until the user presses
  Retry (`ConnectDriver::retry_history`). The flag clears when any message
  lands or the window resets. `GetHistoryAround` failures, which previously
  left an endless skeleton, are covered too.

## Verified

- `src/connect/tests/flood_retry.rs`: retry-after parsing, notice text, retry
  scheduled and re-sent with the same extra, no duplicate while waiting, cap
  and surfacing after 3 retries, over-long waits surface, user-action notice
  without retry, background lookups stay quiet, failed send keeps
  `can_retry`, sweep per purpose (history swept, download and send kept),
  offline suspends the sweep, flood-waiting entries exempt, history retry
  state and `retry_history`.
- `quill-tools/gate.sh`.

Not verified: no live account, so no real 429 from Telegram; the UI Retry row
was compiled but not screenshotted.
