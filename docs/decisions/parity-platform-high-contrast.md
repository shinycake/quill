## Slice: platform-high-contrast (2026-09-30, loop 1)

- **Scope:** a real high-contrast theme/mode for the whole app.
- **What was built:**
  - `ThemeChoice::HighContrast` (third Appearance theme; serde
    `"high_contrast"`, backward-compatible — old prefs files still load).
  - `chat_theme`: orthogonal `HIGH_CONTRAST` flag + 3-value `pick()`
    (`Some(hc)` overrides; `None` keeps the dark value in HC — semantic
    hues were designed for black backgrounds, so only surfaces/text/
    borders need overrides). Palette: pure-black surfaces, white
    text/borders, yellow links (`accent`), deep-blue fills
    (`accent_strong`, white text stays legible), dark semantic fills.
  - `apply_appearance`: HC pairs with the dark kit theme (the kit has no
    HC variant) and wins over auto-night — an explicit accessibility
    choice is never silently reverted by the schedule.
  - UI: third radio in the Appearance theme section; View → Toggle Theme
    cycles Light → Dark → High contrast.
  - `QUILL_DEMO_THEME=high-contrast` for screenshot demos.
  - Tests: `ThemeChoice` serde round-trip + legacy-prefs compat
    (settings.rs); palette contract test (chat_theme.rs, ui-gated).
  - README box `parity:platform-high-contrast` declared via
    `parity-fragments/parity-platform-high-contrast.txt` (merge pipeline
    checks the box after merge).
- **Key decisions (ponytail):**
  - HC as a third `ThemeChoice`, not an orthogonal toggle: the kit
    `ThemeMode` enum is closed (Light/Dark), and HC supersedes the
    light/dark choice anyway.
  - Did NOT recolor every semantic token: dark-mode semantic values
    already have good contrast on black; only surfaces/text/borders got
    HC overrides (~24 tokens, the rest pass `None`).
  - Incoming bubbles stay very dark gray (`0x1a1a1a`) — bubble text is
    hardcoded white, so a white HC bubble is impossible without
    restyling the kit Message component (out of scope); the edge is
    subtle, noted as a limitation.
- **Out of this slice:** kit-component HC variants (kit has none —
    forced dark); per-component HC restyling (bubble borders, etc.).
