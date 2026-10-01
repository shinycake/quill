## Parity slice — paste image from clipboard (2026-09-30)

- **Scope:** `parity:platform-paste-image`. Ctrl+V/Cmd+V with an image on the clipboard now attaches it as a photo to the composer instead of pasting nothing.
- **Key decisions (ponytail):**
  - The kit Textarea's internal paste is text-only (`clipboard.text()`), so rather than forking the kit, the slice registers a `Paste` action handler on the app root that bubbles after the Textarea's own handler. It no-ops unless the composer textarea has focus (same gate pattern as the formatting shortcuts), so search boxes and dialogs keep plain text paste.
  - `ClipboardEntry::Image` already carries encoded bytes (`Image { format, bytes }`) — no re-encoding needed; bytes are written to a temp file (`quill-paste-<nanos>-<pid>.<ext>`) and picked through the existing `ComposerAttachment::pick` path, so the send-path canonicalization and album rules apply unchanged.
  - The lib helper (`clipboard_image_attachment`) takes a plain extension string, not `gpui::ImageFormat` — `src/composer.rs` builds without the `ui` feature and can't reference gpui types. The format→extension match lives in the UI layer.
- **Tests:** `clipboard_image_attachment_persists_png_bytes`, `clipboard_image_attachment_names_are_unique` (nanos+pid naming; millis could collide on rapid successive pastes).
- **Proof:** `docs/screenshots/ready-paste-image.png` — GPUI `--screenshot-demo ready-paste-image` (composer with pasted clipboard photo attachment chip; Xvfb+ffmpeg capture).
- **Out of this slice:** drag-and-drop files into the composer (`parity:platform-drag-drop-files`) — separate input path.
