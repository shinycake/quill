# codex/gif-panel — tidier GIF picker

- Save / "Remove from saved GIFs" moves from a text button under every tile into the tile's
  right-click menu, matching sticker favorites (#329). Removal still uses the shared confirm
  dialog.
- Saved / Trending are small toggle buttons; the active one is shown `selected`.
- The search button is an icon button.
- The status line appears only for loading, error and empty states. The always-on "Tap a GIF to
  send it." hint is gone, and the empty saved state points at the right-click action.
- Tiles without a thumbnail use the muted fill rather than the accent fill.
