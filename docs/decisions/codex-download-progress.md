# codex/download-progress — media status disc

- Photo, video and GIF frames get one shared, centered status disc (`media_disc`): a dark
  translucent 48px circle with a download arrow, a kit `ProgressCircle` while downloading
  (determinate from `ParsedFile::download_progress`, indeterminate when the total is unknown),
  or play/pause.
- Videos and GIFs drop the separate Play/Pause button under the frame; the disc is the control.
  Its click stops propagation, so a click elsewhere on the frame still opens the viewer. While a
  clip plays, the pause disc shows only on hover (`MEDIA_VISUAL_GROUP`).
- Placeholders lose their "— not downloaded" text and green/accent fills in favor of the muted
  media fill. Secret and spoiler media keep their text labels. The video's "Video" corner tag is
  dropped (the duration pill stays). The GIF tag stays, restyled as a translucent pill.
- Embedded icons gained Play, Pause and ArrowDown.
