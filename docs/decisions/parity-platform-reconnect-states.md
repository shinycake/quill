## Slice platform-reconnect-states — PER-STATE RECONNECT LABELS (2026-09-30)

**Scope:** `parity:platform-reconnect-states` — completes the deferred half of the merged #218 (`platform-offline-indicator`). #218 rendered the offline banner ("Waiting for network…") but reduced every transitional state to a bare presence dot. This slice gives each transitional state its label: "Connecting…", "Updating…", "Connecting to proxy…".

- **Built:**
  - `src/state/session.rs`: `ConnectionIndicator::Transitioning` now carries the per-state label (`Transitioning(&'static str)`); `connection_indicator()` maps each `ConnectionState` 1:1 (`ConnectingToProxy` → "Connecting to proxy…", `Connecting` → "Connecting…", `Updating` → "Updating…"; `Unknown` falls back to "Connecting…"). New `ConnectionIndicator::label()` is what the strip renders.
  - `src/ui/app_render.rs`: the transitional arm renders the presence dot + `text_sm` muted label side by side (centered, 6px gap), still a slim strip — no more dot-only.
  - Tests (`src/state/tests/connection_indicator.rs`): assert every state → indicator + label, the `label()` accessor, and the live `updateConnectionState` → "Updating…" path.
  - Screenshot: new `ReadyReconnecting` demo variant (`"ready-reconnecting"`, fixture forces `ConnectionState::Updating`) at `docs/screenshots/ready-reconnecting.png`.
  - Declared via `parity-fragments/parity-platform-reconnect-states.txt` (merge pipeline checks the README box).
- **Key decisions (ponytail):** the label rides inside the existing enum rather than a second parallel mapping — one `connection_indicator()` call still answers visibility, style, and text. `Unknown` gets the generic "Connecting…" fallback (it's not a real TDLib state with its own label; the README lists only the four real labels).
- **Not verifiable without live Telegram:** real TDLib-driven transitions between the states against a live account.
- **Out of this slice:** retry-now affordance on the strip; connection-quality/proxy-latency details.
