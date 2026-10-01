## Parity slice — Story custom-emoji / paid reactions render + picker (2026-09-30)

- **Scope:** finish `parity:stories-custom-reactions` (was partial: envelope already parsed `reactionTypeCustomEmoji`/`reactionTypePaid` into story state, `setStoryReaction` custom-emoji builder + driver existed; viewer render + picker offer were pending).
- **Built:**
  - Envelope (`telegram/envelope/stories.rs`): `StoryAvailableReactionView` now carries `StoryAvailableReactionKind::{Emoji, CustomEmoji(i64), Paid}` — `parse_story_available_reaction` keeps all three rows (custom/paid were dropped).
  - Requests: `get_story_custom_emoji_stickers` (`getCustomEmojiStickers`, schema 1.8.67 `:14751`).
  - State: `RequestPurpose::GetStoryCustomEmojiStickers`; `Session::story_custom_emoji_stickers: HashMap<i64, StickerItem>` (keyed by sticker id = custom emoji id); purpose-gated reducer arm (S10's emoji-panel `GetCustomEmojiStickers` slot untouched).
  - Driver (`connect/stories.rs`): `maybe_fetch_story_custom_emoji_stickers(ids)` — dedupes vs cache, one in-flight request.
  - UI (`ui/story_viewer.rs`): picker offers emoji (as before) + custom-emoji tiles (sticker `img()` with ✨ fallback, Premium badge on `needs_premium`; tap → `pick_story_custom_emoji_reaction` → `set_story_custom_emoji_reaction`); paid never offered (`setStoryReaction` can't set paid — schema comment `:13809`). Action row shows the chosen custom-emoji sticker (✨ fallback) or ⭐ for paid. Viewer tick keeps the stickers warm while open (picker customs + chosen custom).
  - `StickerItem::display_file_id` (thumbnail-first, else static WEBP) — same rule `StickerContent` already had.
  - Declared in `parity-fragments/parity-stories-custom-reactions.txt` (merge pipeline checks the README box).
- **Key decisions (ponytail):** reused S10's `stickers` parse + the composer sticker picker's `img()`-with-fallback pattern instead of new render infra; no user-premium state exists in session, so Premium enforcement stays server-side (picker badges `needs_premium`, TDLib rejects, error surfaces on the request) per the existing driver comment; animated (tgs/webm) custom emoji show their thumbnail/static frame only — no animation player in this slice.
- **Tests:** parse keeps all three kinds; driver fetch dedupes in-flight + caches; updated the two assertions on the old emoji-only shape.
- **Out of this slice:** `parity:stories-live-play`; custom-emoji animation playback.

- **Rebase fix (2026-09-30):** while the viewer is open, `ensure_story_custom_emoji_downloads` uses the automatic-download driver API to warm `downloadFile` for each cached custom-emoji sticker's `display_file_id` (skips usable / already-active downloads). This avoids adding display chrome to the user download list. Metadata from `getCustomEmojiStickers` alone left the `img()` picker/badge path dead (✨ fallback forever).
- **UI proof:** `docs/screenshots/ready-story-post.png` — GPUI `--screenshot-demo ready-story-post` (viewer + reaction picker with emoji rows and a custom-emoji Premium tile on a local sticker thumb; Xvfb capture). Copied to `/workspace/quill-ceo/docs/screenshots/ready-story-post.png` and `/workspace/quill-ceo/ready-story-post.png`. The custom tile reserves room for its 36px thumbnail and an unwrapped Premium label.
- **Regression coverage:** the custom-emoji driver cache test follows metadata ingestion with `downloadFile`, verifies in-flight deduplication and no user download-list entry, then ingests the completed file and checks its usable local path and completed-file deduplication.
