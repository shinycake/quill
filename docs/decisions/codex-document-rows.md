# codex/document-rows — document rows with an action disc

- Documents render as a row (after tdesktop's `HistoryDocument`): a 44px round disc, then the
  name and a compact meta line. The old bordered box nested inside the bubble is gone.
- The disc is the primary action: download (arrow), cancel (✕ inside a kit `ProgressCircle`
  ring, determinate when the size is known), open (file glyph) or retry. The name triggers the
  same action.
- The meta line reads "450 KB · PDF" / "1.2 MB of 3.4 MB" / "Download failed" and carries the
  secondary actions as link-colored text: Show in folder, Retry, Pause/Resume (only for listed
  user downloads, as before).
- On outgoing bubbles the disc is translucent white and the links inherit the bubble color.
  `document_chip` gains an `outgoing` parameter; sponsored rows pass `false`.
- `document_kind_label` derives the type tag from the extension, else the MIME subtype (tested).
