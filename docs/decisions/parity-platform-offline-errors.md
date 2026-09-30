## Slice parity/platform-offline-errors (2026-09-30)

**Scope:** `parity:platform-offline-errors`. Builds on merged `parity:platform-offline-indicator` (#218: `Session::connection` → indicator strip).

- **Built:**
  - State (`state/session.rs`): `Session::is_offline()` — true for any `connection != ConnectionState::Ready`.
  - UI (`ui/composer.rs`): new `App::send_started_note(online_note)` helper — when offline, the send/edit success note becomes "You're offline — will send when you reconnect" instead of "sending…"/"saving edit…". Wired into all three send paths (`submit_composer`, `submit_rich_composer`, `submit_edit`). Sends still go through: TDLib queues outgoing requests while offline and flushes them on reconnect, so the note claims queueing, not failure.
  - UI (`ui/calls.rs`): `dial_user` refuses while offline — "You're offline — can't start a call" (a call can't be queued, so it blocks; covers both the direct and confirm-dialog paths).
- **Key decisions (ponytail):** no new state — reads the existing `Session::connection` that the indicator slice already maintains; demo scenarios unaffected (they set `connection` explicitly per scenario). Didn't block sends — blocking would be dishonest since TDLib delivers them on reconnect.
- **Tests:** `is_offline_follows_connection_state` (state).
- **Out of this slice:** none identified.
