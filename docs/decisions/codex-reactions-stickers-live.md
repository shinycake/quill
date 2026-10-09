# Reactions, stickers and emoji: live data (batch B11)

## What tdesktop does

Read from `history/history_inner_widget.cpp` (`mouseDoubleClickEvent`,
`toggleFavoriteReaction`), `chat_helpers/emoji_keywords.cpp`,
`history/view/history_view_about_view.cpp` (`GenerateChatIntro`), and the
reaction and sticker data classes in `data/`.

- Double-click on a message with the quick action set to React (the default)
  toggles the favorite reaction (`reactions().favoriteId()`). It does nothing
  when the reaction is not among the message's possible ones, and it only
  fires when the cursor is not over text or a link. The favorite reaction is
  chosen in Settings (`boxes/reactions_settings_box.cpp`) from the list of
  active reactions.
- Recent, favorite and trending stickers, installed sets and the active and
  per-chat reactions are server state: when another device changes them an
  update arrives and the open panels follow.
- Emoji search also matches keyword packs of every language in use
  (`EmojiKeywords`): the app language plus the keyboard layouts.
- An empty private chat shows "No messages here yet..." with a random hello
  sticker; clicking the sticker sends it.
- Reacting plays a fly animation (`ReactionFlyAnimation`).

## What changed in Quill

- Updates handled (`src/telegram/envelope/payload.rs`, `src/state/session_apply.rs`):
  `updateRecentStickers` (not the attached variant), `updateFavoriteStickers`,
  `updateTrendingStickerSets` (regular and custom emoji),
  `updateActiveEmojiReactions`, `updateChatAvailableReactions`,
  `updateDefaultReactionType`. `updateInstalledStickerSets` was already
  handled. The reducer only sets stale flags; the driver
  (`ConnectDriver::refresh_stale_panels`, `src/connect/stickers.rs`) refetches
  lists that are loaded or whose panel is open, so a closed panel costs
  nothing. The open reaction picker refetches its options when the active
  reactions or its chat's allowed reactions change.
- Quick reaction: Settings > Media settings > "Quick reaction" lists the active
  reactions (`setDefaultReactionType`, optimistic, confirmed by the update).
  Double-click on a message row reacts with it (`QuillApp::quick_react`);
  it is skipped while selecting messages or when text is selected.
- Who reacted: the audience page has an "All N" tab and one tab per reaction
  (`getMessageAddedReactions` with `reaction_type`), each paged 50 at a time
  with a "Show more" row. Pages append and keep the next offset. Not done:
  the hover tooltip over a reaction chip.
- Fly animation (`src/ui/history_fx.rs`): the added emoji pops, drifts up and
  fades over 520 ms. It runs on the shared frame clock only while it plays,
  never `with_animation`, and is skipped when the OS asks for reduced motion.
- "Remove from recent" on a recent sticker (`removeRecentSticker`, the list
  drops it at once) and on a recent emoji, and "Reset recent emoji" (the
  recent emoji list is local in Quill, so it clears `MediaPrefs`; TDLib has
  no such method).
- Keyword search: the emoji panel search also calls `getKeywordEmojis`; the
  language codes come from the typed script, the system locale and English
  (`keyword_language_codes`). Reaction search uses the same matches.
- "Attached Stickers" in the media viewer menu for photos and videos with
  `has_stickers` (`getAttachedStickerSets`, file id of the media). Only the
  first returned set opens in the existing sticker set dialog; tdesktop lists
  all of them in one box.
- Greeting sticker: an empty private chat (not a bot, not Saved Messages)
  shows the intro text and a hello sticker from `getGreetingStickers`,
  asked once per session. The pick is stable per chat; clicking sends it.

## Skipped

- Per-chat available reactions UI (group admin settings), paid reactions.
- Double-click-to-reply setting and corner buttons.
- Hover tooltip of who reacted.

## How verified

- `connect/tests/reactions_live.rs`: refetch rules for every update, default
  reaction round trip, picker refetch, reactor tab paging, remove recent,
  keyword search languages, greeting and attached sets.
- Request shape tests (`tests_stickers.rs`), language code tests (`emoji.rs`),
  fly curve tests (`history_fx.rs`).
- Demo fixtures: `QUILL_DEMO_MENU` scenarios now carry reaction tabs and a
  default reaction.
- `gate.sh` passes. No live account was used; the double-click and fly
  animation were not exercised on a screen.
