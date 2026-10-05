# Message reactions: menu strip, every allowed reaction, custom emoji

## Problem
The "Add reaction" picker offered a hardcoded list of 16 emoji, whatever the
chat allowed. Custom-emoji reactions were invisible under messages. Custom
emoji in message text resolved to stickers, but their image files were never
downloaded, so they always fell back to plain emoji.

## Decision
- Reactions lead the message menu, as in Telegram Desktop. Opening the menu
  asks `getMessageAvailableReactions`. The strip shows the chat's top
  reactions; the chevron expands it to every allowed reaction (top, recent,
  popular). When the chat allows custom emoji and the account is Premium, the
  expanded grid also shows recently used custom emoji. Clicking a cell
  toggles that reaction and closes the menu.
- Chips under messages render emoji and custom-emoji reactions alike
  (`HistoryMessage::reaction_chips`). Custom glyphs come from the
  `getCustomEmojiStickers` cache.
- The driver now downloads the image files of resolved custom emoji that the
  open chat uses (`Session::open_chat_custom_emoji_files`). This covers
  message text, reactions and the picker. It fixes custom emoji in message
  text too.
- Single-codepoint emoji get U+FE0F for display, so TDLib's "❤" draws in
  colour.
- The old `pending_react` picker panel is removed.

## Not yet
Whole custom-emoji packs in the expanded grid (only recents for now); paid
reactions; reaction tags in Saved Messages.
