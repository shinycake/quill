# Installed sticker set ordering

Installed set rows use the existing GPUI drag/drop pattern. Dropping a set onto another row moves it to that slot and sends the complete regular-set order through `reorderInstalledStickerSets`. The driver rejects missing, nonpositive or duplicate IDs, skips a no-op drop, and serializes reorders against catalog mutations and refreshes. The displayed catalog stays intact on errors; a confirmed operation invalidates older pending fetches and reloads TDLib's authoritative order.

Validation: the focused driver regression checks both drag directions, the full request order, invalid inputs, concurrent mutation/reorder suppression, failure preservation, and successful authoritative reload. Core clippy and UI compilation pass. Live Telegram drag interaction is not yet verified.
