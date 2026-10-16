# One path settles a lost request

## Problem

A request can end without an answer in three ways, and each handled it on its
own:

- The periodic sweep (`Session::sweep_stale_requests`) dropped a stale lookup
  but only settled `GetHistory`, `GetHistoryAround` and `GetHistoryNewer`.
  `RequestPurpose::is_sweepable` covers many more (shared-media and in-chat
  search pages, topic and thread history, pinned messages, full info). Their
  `loading_more` flags stayed true and blocked every later page request.
- The flood-retry re-send (`request_tick`) dropped the pending entry with
  `requests.take` when the send failed, so the history window kept its
  skeleton instead of the Retry row.
- Account loss (`RequestRegistry::invalidate_account`) cleared the registry
  but not the stored flags (profile playlist, network statistics, Stars
  subscriptions, Stars transactions, gifts). The fetchers return early on
  those flags, so reopening the panel never retried. It also left
  `stale_history_requests` behind.

tdesktop has no equivalent: MTProto sessions resend or fail every request
(`MTP::Sender` fail handlers), so a request never silently disappears.

## Change

`Session::settle_lost_request(pending)` (in `session_apply_error.rs`) feeds the
removed request through `apply_payload_in_asking_view` as an error with code
408. Each domain's existing failure handler (`apply_media_error`,
`apply_search_error`, `apply_threads_error`, `apply_payments_error`, ...) then
clears its own flags exactly as it does for a TDLib error, in the window view
that asked.

- Sweep: every dropped sweepable request is settled this way. The special
  cases for history in the sweep are gone; the messages error handler already
  has them. The group-call mute toggle (swept without error) is only dropped.
- Flood retry: a failed re-send settles the request instead of taking it.
- Account loss: `Session::settle_account_requests` drains the registry and
  settles every pending lookup (anything `is_user_action` does not classify as
  a user action), then clears `stale_history_requests`. User actions are
  dropped without a failure notice about an account the user left.
  `begin_logout` and the `LoggingOut` / `Closed` authorization states use it.

Flags were not replaced by "is a request pending" derivations: the flags are
read in many panels and set by tests, and the failure handlers already own
their reset. The invariant now lives in one function instead.

## Verified

State tests (`src/state/tests/lost_requests.rs`): a swept shared-media page
allows the next page, a swept in-chat search page clears `loading_more`, every
sweepable purpose leaves the registry, and logout resets the five stored
flags and the stale set. Driver test
(`src/connect/tests/flood_retry_lost.rs`): a flood-parked history request
whose re-send fails sets the history Retry state.
