# codex/ingest-budget — bounded main-thread update drain

- `poll_live` drained the whole TDLib receive queue in one go on the UI thread. A sync burst
  (reconnect, first login, a busy group) could freeze a frame for as long as the reducer took
  over thousands of updates.
- The drain now stops after `INGEST_BUDGET` (8 ms, half a 60 Hz frame). Leftovers stay in the
  channel, and the poll loop returns `busy`, so the next tick runs at the 10 ms busy cadence.
- Logout detection already latches `LoggingOut` across ticks (`prev_auth`), so a burst split
  between ticks still restarts correctly. Parsing was already off-thread (receive bridge).
