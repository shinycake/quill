# Emoji set style picker

Parity item `emoji-set-style`, marked blocked in `docs/decisions/
codex-emoji-set-style.md` ("needs bitmap-sprite emoji glyphs in GPUI text
layout"). Architecture pass 2026-10-14: the blocker is gone, because Quill
now draws custom emoji as inline images over an invisible placeholder
glyph in the same `StyledText` paragraph (`src/ui/selectable_text.rs`
`InlineEmoji`, `EMOJI_PLACEHOLDER`; `src/ui/message_text/
rich_text_reserving.rs` `inline_paragraph`). Standard emoji can use the
same path. Copy to `docs/decisions/emoji-set-style.md` on the build
branch; fill in "How verified".

## What tdesktop does

- Settings → Chat Settings → "Choose emoji set" (`lng_emoji_manage_sets`,
  `chat_helpers/emoji_sets_manager.cpp` `ManageSetsBox`): four rows with a
  preview strip of six emoji each: "Mac" (built in, id 0), "Android"
  (7.9 MB), "Twemoji" (6.1 MB), "JoyPixels" (7.8 MB). Row state text:
  "Download {size}" → "{percent}, {progress}" while loading → "Downloaded"
  → "Current set" (`lng_emoji_set_*`). Clicking a downloaded set switches
  immediately; every emoji in the app re-renders from the sprites.
- A set is a zip (`storage/storage_cloud_blob.cpp` `UnpackBlob`) holding
  `config.json` and `emoji_N.webp` sprite sheets (`lib_ui/ui/
  emoji_config.cpp` `LoadSprites`, `SetIsReady`), fetched over MTProto
  from the `@tdhbcfiles` channel posts 3223–3225 with a dedicated loader.
  The emoji → (sheet, cell) index is tdesktop's generated emoji table.

## Decisions

1. **Quill ships its own sprite packs**, built by a script in `quill-tools/
   emoji-sets/` from openly licensed art and published as release assets of
   `shinycake/quill` (`emoji-set-<name>-<version>.zip`): "Noto" (Google Noto
   Color Emoji, Apache-2.0; tdesktop calls it "Android") and "Twemoji"
   (CC-BY 4.0, attribution in `THIRD_PARTY_LICENSES.md` and in the box's
   footnote). JoyPixels is not free for redistribution and is left out
   (documented deviation). The system set stays the default and is named
   after the platform: "Apple" on macOS, "Segoe" on Windows, "System" on
   Linux. Fetching tdesktop's blobs from `@tdhbcfiles` is not used: the post
   ids are tdesktop's private update infrastructure and the index order
   inside them is tdesktop's GPL table.
2. **Pack format** (ours, versioned in `config.json`): `{ "version": 1,
   "cell": 72, "columns": 32, "sheets": N, "unicode": "17.0", "index":
   { "<emoji without FE0F>": [sheet, cell] } }` plus `emoji_1.webp` …
   `emoji_N.webp` (lossless WebP, 32×32 cells of 72 px, ≈ 6 MB per set).
   The index is keyed by the emoji string from Quill's own catalog
   (`assets/emoji/emoji-17.tsv`, Unicode data license), so no tdesktop
   order is needed. Skin-tone and ZWJ sequences are entries of their own;
   a missing entry falls back to the system glyph.
3. **Rendering** reuses `InlineEmoji`: when a non-system set is active,
   text runs are split at emoji grapheme boundaries (the same segmenter as
   `emoji_catalog::big_emoji_count`, `unicode-segmentation`) and each
   emoji becomes an `EMOJI_PLACEHOLDER` with an `InlineEmojiVisual::Sprite
   { sheet, cell }` painted from the sheet texture (one `RenderImage` per
   sheet, cached in the image budget as an app-wide entry; the cell is a
   sub-rect through `img(..).object_fit(..)` is not enough, so paint the
   sheet with a content mask over the cell box, as the layered custom
   emoji clip already does). Copy and selection keep the real characters
   (`with_emoji_fallbacks` already maps placeholders back).

## Coverage

Where sprites show, in this order of implementation:

1. Message text and captions, reply quotes, link-preview text
   (`message_text/`), reactions (`ui/reactions.rs`), chat-list last message
   and titles (`ui/chat_row/`), the emoji panel (`ui/media_panel/` emoji
   tab, where it also serves as the set preview), big-emoji messages
   (scaled cell, same as custom emoji "large" path).
2. The composer: the kit `Input`/rich input renders glyphs itself and has
   no inline-image hook (`src/rich/input.rs` treats custom emoji as plain
   text today). The composer keeps system emoji. This is a documented
   deviation; it also matches what the composer already does for custom
   emoji.
3. Titles in dialogs, toasts and settings stay system glyphs (tdesktop
   replaces them too; low value, documented).

## Design for Quill

### Core (`src/emoji_sets_style/`, lib crate; each file < 1000 lines)

- `catalog.rs`: `EmojiSetId { System, Noto, Twemoji }`, display names per
  platform, sizes, download URL = `https://github.com/shinycake/quill/
  releases/download/emoji-sets-v1/emoji-set-<name>.zip` (pinned tag
  constant like `spell_catalog::CATALOG_REV`), SHA-256 per pack (constants
  checked after download; a mismatch deletes the file).
- `pack.rs`: `SpritePack { cell, columns, sheets, index: HashMap<Box<str>,
  (u8, u16)> }` parsed from `config.json` (serde, `deny_unknown_fields`
  off, version check), `lookup(grapheme) -> Option<(sheet, cell)>` that
  strips `\u{fe0f}`; zip unpack with the `zip` crate (already in the lock
  through other deps? if not, add `zip` with default features off,
  deflate only) into `<app data>/emoji-sets/<name>/`, path-traversal
  guard like tdesktop's `cleanPath` check; `is_ready(name)`.
- `state.rs`: `EmojiStyleState { current: EmojiSetId, downloads:
  HashMap<EmojiSetId, DownloadState { Available(bytes) | Loading
  { done, total } | Ready | Failed(String) } }`, persisted in
  `settings::MediaPrefs` (`emoji_set: String`), loaded at startup; the
  active pack is parsed on a background thread and handed to the UI as
  `Arc<SpritePack>`.
- `download.rs`: HTTP download through the same mechanism the spell
  dictionaries use (`spell_dict.rs` warm path: `thread_guard::
  spawn_guarded` + the existing HTTPS client), with progress callbacks
  for "{percent}, {progress}"; cancel on box close like tdesktop's
  `Loader` destructor.

### UI

- `ui/emoji_style_ui.rs` (`register_dialogs!`): "Choose emoji set" box
  mirroring `ManageSetsBox` with kit components: a `Dialog`, one row per
  set with a six-emoji preview strip (😀 😂 🥰 😎 🤔 🚀 rendered from the
  set's own sprites once downloaded, system glyphs otherwise, as tdesktop
  draws its `PreviewPath` images), the state line ("Download 6.1 MB",
  "42%, 2.5 MB / 6.1 MB", "Downloaded", "Current set"), a `Progress` bar
  while loading, `IconName::Check` on the current set, Escape closes.
  Entry: Settings → Chat Settings row "Choose emoji set" next to
  "Replace emoji automatically".
- `ui/emoji_sprites.rs`: `SpriteAtlas` (one `Arc<RenderImage>` per sheet,
  decoded lazily from the WebP via the existing `image` webp feature,
  retired through `image_budget::retire_all` on set change) and
  `InlineEmojiVisual::Sprite` painting in `selectable_text.rs` (cell
  sub-rect via content mask; `EMOJI_SCALE` reused).
- Splitting plain runs into emoji runs happens once per message in the
  text-run builder (`message_text/mod.rs` run construction), guarded by
  `current != System` so the system path costs nothing.
- Set change: swap the `Arc<SpritePack>` in app state, retire old sheet
  images, `cx.refresh()`; chat-list and history caches that hold laid-out
  text are invalidated by bumping the existing text cache generation.

### Tests

- Unit: `config.json` parse + version check; `lookup` with and without
  FE0F, skin tones, ZWJ; zip unpack rejects `../` names; SHA mismatch
  deletes; run splitting at emoji graphemes (mixed text, flags, keycaps).
- Driver-free (no TDLib involved). Download tested with a local file URL
  / mocked fetch like `spell_dict` tests.
- UI: the box lists three sets with the system one current; selecting a
  ready set re-renders a demo message with `InlineEmojiVisual::Sprite`
  (assert placeholders in the laid-out text); copy of a sprite message
  yields the original characters.
- Visual: captures of the box (three states) and a chat with Twemoji,
  light and dark.

### Tooling (`quill-tools/emoji-sets/build.py`)

Reads `assets/emoji/emoji-17.tsv`, pulls Noto (`googlefonts/noto-emoji`
PNG 72 px) and Twemoji (`jdecked/twemoji` SVG → raster at 72 px with
`resvg`), writes sheets + `config.json`, prints coverage (entries missing
in the art), zips, and prints the SHA-256 for `catalog.rs`. Idan uploads
the zips to the `emoji-sets-v1` release (the design notes that the first
build is blocked on that upload; until then the box shows "Download"
failing with the real HTTP error).

## Build plan and model

One PR, Opus 5.5 medium (text-run splitting and sprite painting touch
the shared text pipeline; needs care). ~3,500 lines. Prerequisite: the
release assets exist (tooling first, in the same PR, run locally; the
upload is Idan's step).

## Deviations

- JoyPixels omitted (license); set names follow the art ("Noto" rather
  than "Android").
- Packs come from Quill's GitHub release, not Telegram's channel.
- Composer, dialog titles and toasts keep system emoji.

## What the build changed from this design

- The row preview is tdesktop's 2×2 grid of four faces (😃 😉 😔 😰, the
  same faces as its `setN_preview.webp`), not a six-emoji strip. The
  previews of the packs are bundled PNGs (`assets/emoji/sets/`), so a set
  shows its art before it is downloaded, as in tdesktop.
- The choice is app-wide, like tdesktop's, in `<app data>/emoji-sets/current`
  rather than in the per-account `MediaPrefs`.
- A download keeps going when the box closes (tdesktop's loader is global;
  only starting another set's download replaces it).
- Sheets are 16×16 cells of 72 px (16 sheets per pack), and the atlas keeps
  one small image per emoji of a decoded sheet, so the GPU only gets what
  is painted. A sheet is decoded on first use, on the background executor;
  until then its emoji keep the system glyph.
- Characters that are text by default (Unicode `Emoji_Presentation=No`:
  ©, ®, ™, ↔, ❤…) stay text unless U+FE0F follows them.
- Covered: message text and captions (with big emoji), reply strips, the
  chat-list preview line and titles, reaction chips, the reaction picker
  and quick-reaction strip, and the emoji panel. The composer, dialog
  titles, toasts and the send-effect strip keep system glyphs (documented
  deviation).
- Packs are stored zips (WebP is already compressed), so the `zip` crate
  is used without compression features.

## How verified

- Packs: `quill-tools/emoji-sets` built both from Noto Emoji
  (googlefonts/noto-emoji `e20cbc2b`, flags from region-flags rendered with
  resvg) and Twemoji (jdecked/twemoji `b6b55fef`): 3,944 emoji each, none
  missing, 4,464,013 and 3,107,720 bytes. The ignored test
  `built_packs_match_the_catalog_and_install` (`QUILL_EMOJI_PACKS=<out>`)
  checked size, hash, unpacking and lookups of flags, keycaps, skin tones
  and ZWJ families against the real zips.
- Unit tests: catalog states and strings, saved choice, pack parsing and
  version check, unpacking (traversal names skipped, incomplete packs
  refused, old pack kept), hash mismatch, URL policy, text splitting.
- GPUI test `switching_sets_draws_emoji_from_sprites`: switching to an
  installed fixture pack saves the choice and makes message emoji pictures
  once the sheet decodes; switching back returns to the system font.
- Demo captures (`ready-emoji-sets`, `ready-emoji-set-chat`, `ready-reactions`
  with `QUILL_DEMO_EMOJI_PACKS`/`QUILL_DEMO_EMOJI_SET`) in light and dark,
  with the system font, Noto and Twemoji.
- Not verified: the real download from GitHub, because the release assets
  are not uploaded yet. Until they are, a download shows "This emoji set
  isn't available yet."; Linux and Windows were only built by CI.
