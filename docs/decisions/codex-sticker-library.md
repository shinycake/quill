# codex/sticker-library — data layer for the emoji/sticker panel and reactions

Groundwork for the Telegram-Desktop-style emoji/sticker/GIF panel and the reaction picker
(the UI follows in its own PR).

## Why stickers didn't load
The panel showed one set at a time, and nothing ever requested the sticker files: a cell
only showed art if its file happened to be local already.

## Added
- `Session::media_library` holds the contents of each installed sticker / custom-emoji set
  (`getStickerSet` under `RequestPurpose::LoadLibrarySet { set_id }`). Sets load lazily as
  their sections come into view: at most `MAX_LIBRARY_LOADS` (4) in flight, loaded and loading
  sets are skipped, and failed sets aren't retried automatically.
- `ConnectDriver::open_media_panel` requests the installed regular and custom-emoji set lists,
  recent stickers and favorites, each once.
- `ConnectDriver::ensure_library_sets` loads the contents of the sets whose sections are on
  screen. `ConnectDriver::ensure_media_files` downloads the display files of on-screen cells
  (priority 8, deduped by `should_download`).
- Reactions: `getMessageAvailableReactions` (`RequestPurpose::GetMessageAvailableReactions`).
  The shared `availableReactions` parser now keeps `recent_reactions`, `popular_reactions` and
  `allow_custom_emoji`, and the reducer routes message answers to
  `Session::message_reaction_options` (stories unchanged). `ReactionChoice` covers emoji and
  custom emoji. `ConnectDriver::toggle_reaction_choice` adds or removes either kind
  (`set_message_reaction`).
- Custom emoji in the composer: `FormatKind::CustomEmoji` parses Telegram's
  `![😀](tg://emoji?id=N)` markup into `textEntityTypeCustomEmoji` over the fallback emoji
  (`composer::custom_emoji_markup`). Rich text keeps the fallback.

## Tests
- Driver tests cover panel open dedupe, lazy loading with the in-flight cap, failure without
  retry, downloads, reaction options parsing, and custom-emoji reaction add/remove.
- A markup round-trip test.
