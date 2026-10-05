# codex/downloads-panel — round video messages and the Downloads panel

## Video messages
- Round video messages stand on their own, without a bubble (the `plain` look stickers use),
  as a 220 px circle.
- The shared centered media disc replaces the "Video note · playing" / "— not downloaded" text
  labels and the separate Play/Pause button. It shows play/pause, or a download ring while
  loading, and stays hover-only while playing. The whole circle toggles playback.
- A duration pill sits at the bottom, with a dot while unseen. An unseen incoming note gets an
  accent ring.
- GPUI clips overflow to rectangles, so the image and placeholder round themselves.
- Outgoing notes right-align their Transcribe link.

## Downloads panel
- Rows match document rows: a 40 px action disc, the name, and a details line.
  - The disc pauses or resumes inside the progress ring (frozen while paused, spinning when the
    size is unknown), retries a failed download, or opens a finished file.
  - The details line reads "42% · 10 B of 24 B", "Paused · …", "Download failed" (in danger
    color), or the size.
  - Cancel and "Show in folder" are small icon buttons.
  - Rows highlight on hover.
- Replaces the text-link actions and bare progress bars. `FolderOpen` is embedded.
