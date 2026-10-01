## Slice parity/platform-flood-errors (2026-09-30)

**Scope:** `parity:platform-flood-errors` — Flood/rate-limit errors with retry countdown.

Flood errors were already classified (`ErrorClass::Flood`) but surfaced only generic "wait and try again" lines.

- **Built:**
  - Envelope (`telegram/envelope/auth.rs`): `TdError` gains `flood_wait_secs: Option<u64>` — extracted from `FLOOD_WAIT_<n>` in `parse_error` (the only place the raw message is still available before secret-scrubbing drops it); only the number survives. New `TdError::flood_line(generic)` helper: "try again in N seconds" when known (singular "1 second"), else the generic line.
  - State (`state/requests.rs`): `password_op_error_line`, `sessions_error_line`, and `error_reason` Flood arms now use the countdown (precomputed local keeps the other `&str` arms unchanged where needed).
  - Auth (`state/requests.rs` + `session_apply_error.rs`): `AuthRequestError` carries `flood_wait_secs` through from `TdError`; `user_message()` returns `String` — "… try again in N seconds" when known (base line minus the generic wait tail), static line otherwise. Production call site `ui/notifications.rs` unchanged (`.into()` still works).
- **Key decisions (ponytail):** no new request or state — the seconds ride the existing error parse; `TdError` stays `Clone` (it was never `Copy`; only `ErrorClass` is). Two existing tests asserted the old static text for `FLOOD_WAIT_3` — updated to the countdown (intended behavior change). Rebased onto main after `msg-richtext-ai-tools` (#238): `parse_error` keeps the `AICOMPOSE_FLOOD_PREMIUM` classification and still attaches `flood_wait_secs` on every return path.
- **Tests:** `parse_error_extracts_flood_wait_seconds`, `parse_error_without_flood_wait_has_no_countdown`, `auth_flood_error_shows_retry_countdown`; updated 2 existing sessions tests.
- **Out of this slice:** none identified.
