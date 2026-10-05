# codex/settings-density — inline settings actions sized to their rows

- Labeled buttons in the privacy dialog and notification settings ("Block…", "Unblock",
  "Change"/"Hide", mute presets, …) use the kit `small` size. They were default size, which
  read larger than the rows that hold them.
- In the per-chat notifications panel, row labels are body text (`text_sm`, foreground) rather
  than tiny muted captions, and the "Mute for" presets are outline chips.
- "Clear saved payment/shipping info…" uses the quiet danger style from #336 (red text,
  tinted on hover) instead of a filled pink bar. `security::quiet_danger` is now `pub(super)`.
