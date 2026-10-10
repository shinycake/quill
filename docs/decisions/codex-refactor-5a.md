## Refactor 5a: split long files (2026-10-10)

- **Scope:** pure refactor, no behaviour or UI change. Five long files
  become module directories (`foo.rs` → `foo/mod.rs` plus private
  submodules), each file under ~1,000 lines. Call sites are unchanged:
  `mod.rs` re-exports every item under its old path.
- **Visibility rules:**
  - Public items keep their visibility and are re-exported from `mod.rs`
    with the same visibility (`pub use` / `pub(super) use`).
  - A `pub(super)` method or item moved one level deeper becomes
    `pub(in crate::ui)`. That is the same set of callers as before.
  - A private helper that a sibling submodule now needs becomes
    `pub(super)`, which keeps it inside the original module, and
    `mod.rs` imports it privately.
  - Submodules start with `use super::*;`, the existing convention in
    `src/ui`. Paths written as `super::foo::` in moved code are now
    `crate::ui::foo::`, so they still point at the same `ui` modules.
- **One code change:** `ui::media_viewer::media_viewer_overlay` was a
  single 1,139-line function. Three self-contained parts of it (the
  video player panel, the zoom row, and the saved-file toast) are now
  helper methods in `overlay_controls.rs`. Their bodies are unchanged,
  except that `item` is passed by reference and `item.message_id` is
  copied into a local before the download closure captures it.
- **Tests:** each test sits next to the code it covers. Counts are
  unchanged: core 2990, UI 224.

### New layout (lines)

`src/media_viewer.rs` (1,788) → `src/media_viewer/`
- `mod.rs` (492): `MediaViewerKind`, `ViewerSource`, `MediaViewerItem`,
  the `MediaViewer` state machine
- `items.rs` (387): building items from history and profile photos
- `controls.rs` (357): video start decision, controls auto-hide, Delete
  gate, seeking, key actions
- `transform.rs` (465): zoom/pan, orientation, RGBA flip/rotate,
  `fit_within`
- `save.rs` (120): save to Downloads
- `test_support.rs` (21): shared test fixture

`src/composer.rs` (3,211) → `src/composer/`
- `mod.rs` (316): `ComposerSnapshot`, draft-save timing
- `send.rs` (270): Enter-to-send, `SendOptions`, scheduling, send note
- `attachments.rs` (342): picked, dropped and pasted attachments
- `reply.rs` (194): reply drafts and quotes
- `edit.rs` (606): message edits, media replacement, link preview choice
- `actions.rs` (152): delete confirmation, forward drafts
- `markup.rs` (780): formatting markup parse/rebuild/apply, URLs
- `shortcuts.rs` (234): formatting chords, link editing
- `commands.rs` (387): bot command menu, inline query, mentions

`src/ui/media_viewer.rs` (3,390) → `src/ui/media_viewer/`
- `mod.rs` (105): `ViewerExtra`, layout constants, fade helpers
- `navigation.rs` (232): open/close/step, file resolution
- `video.rs` (716): autoplay, frame extraction, native playback, audio,
  media session
- `actions.rs` (641): rotate/flip, share, save, copy, delete, sender,
  album pin
- `playback.rs` (627): seek, keys, full screen, speed, volume, prefs,
  tick, zoom/pan
- `overlay.rs` (863): the overlay render
- `overlay_controls.rs` (313): player panel, zoom row, saved toast

`src/ui/message_media.rs` (2,921) → `src/ui/message_media/`
- `mod.rs` (129): display-path helpers, re-exports
- `layout.rs` (272): widths, frames, corners, the media disc
- `visual.rs` (515): photo, GIF, video
- `video_note.rs` (220), `sticker.rs` (120)
- `audio.rs` (380): voice notes, music, waveform, transcription
- `document.rs` (341): document chips and shared small helpers
- `location.rs` (221): location, venue, map tile
- `cards.rs` (397): contact, paid media, dice
- `surfaces.rs` (292): inline video surfaces, badges, spoiler cover
- `fixtures.rs` (119): demo fixtures

`src/ui/notification_settings.rs` (2,692) → `src/ui/notification_settings/`
- `mod.rs` (76): enums, constants
- `global.rs` (527): badge, desktop notifications, in-app sounds, Reset All
- `mute_menu.rs` (444), `ttl_picker.rs` (170)
- `sounds.rs` (435): per-chat sound/preview/story settings, sound picker
- `scope.rs` (709): scope defaults and exceptions
- `reactions.rs` (286): reaction notifications
- `fixtures.rs` (107): demo fixtures
