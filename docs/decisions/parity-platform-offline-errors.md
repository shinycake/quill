## Slice parity/platform-offline-errors (2026-09-30)

**Scope:** `parity:platform-offline-errors`. Builds on merged `parity:platform-offline-indicator` (#218: `Session::connection` → indicator strip).

- **Built:**
  - State (`state/session.rs`): `Session::is_offline()` — true when connection is neither `Ready` nor `Updating`. `Updating` is live sync (sends/calls work; #218 maps it to Transitioning), so treating it as offline was a false positive.
  - UI (`ui/composer.rs`): new `App::send_started_note(online_note)` helper — when offline, the send/edit/retry success note becomes "You're offline — will send when you reconnect" instead of "sending…"/"saving edit…"/"retrying send…". Wired into `submit_composer`, `submit_rich_composer`, `submit_edit`, and `retry_failed_message`. Sends still go through: TDLib queues outgoing requests while offline and flushes them on reconnect, so the note claims queueing, not failure.
  - UI (`ui/calls.rs`): `dial_user` / `start_call_for_user` / `call_again` refuse while offline — "You're offline — can't start a call" (a call can't be queued, so it blocks; refuse happens before the confirm-before-call dialog).
- **Key decisions (ponytail):** no new state — reads the existing `Session::connection` that the indicator slice already maintains; demo scenarios unaffected (they set `connection` explicitly per scenario). Didn't block sends — blocking would be dishonest since TDLib delivers them on reconnect. `Ready | Updating` stay online so mid-sync does not lie about queueing or block dials.
- **Tests:** `is_offline_follows_connection_state` (state) — covers Ready/Updating online and WaitingForNetwork/Connecting offline.
- **Proof:** `docs/screenshots/ready-offline-toast.png` — GPUI `--screenshot-demo ready-offline-toast` (offline banner + kit toast with the product offline-send note; Xvfb+ffmpeg capture).
- **Out of this slice:** poll send; Connecting* copy nuance (still use the offline note while not Ready/Updating).
