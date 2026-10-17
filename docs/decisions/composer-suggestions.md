# Composer suggestions: group bot commands, caret-aware triggers

## What Telegram Desktop does

- `chat_helpers/field_autocomplete.cpp:670-760`: the `/` list is built from the
  commands of every bot in the chat. Basic groups read `chat->botCommands()`,
  megagroups `mgInfo->botCommands()`, a bot chat the bot's own commands. Basic
  groups list the bots that wrote last first (`lastAuthors`), then the rest.
- `field_autocomplete.cpp:735-747`: when the group has bots, or the typed filter
  has an `@`, rows read `command@username` and the filter matches against that
  string.
- `field_autocomplete.cpp:1380-1420`: each row shows the bot's userpic, the bold
  `/command[@bot]`, then the description; ephemeral commands get an icon.
- `field_autocomplete.cpp:1606-1620, 2033-2050`: choosing a row builds
  `/command@bot` the same way; a click sends it and keeps the text after the
  caret, Tab inserts it with `insertTag`.
- `chat_helpers/message_field.cpp:933-1000` (`ParseMentionHashtagBotCommandQuery`):
  the trigger is read at the caret, over the text before it. `@` needs a
  non-name character before it; `/` is a command only when it is the first
  character of the field (`/cmd@bot` is allowed while typing).
- `history/history_widget.cpp:6531` (`insertBotCommand`): the command goes in
  front of the text and a leading `/command[@bot]` is replaced. `@bot` is added
  only for a bot that is the private-chat peer or the sender of a hovered
  command link, and only when the chat has bots; a private chat has none, so
  panels never add it.

## What Quill did

- Commands only appeared in a chat with a bot. `@` and `/` triggers looked at
  the end of the text, and `/` was accepted after any whitespace.
- `insert_bot_command_text` appended the command and never added `@bot`.
- TDLib's `bot_commands` in `basicGroupFullInfo` and `supergroupFullInfo` was
  not parsed.

## What changed

- Parse `bot_commands` from `supergroupFullInfo`, `updateSupergroupFullInfo`,
  `basicGroupFullInfo` and the new `updateBasicGroupFullInfo`; keep them in
  `BotsState::group_bot_commands`. A basic group's full info is requested when
  the chat opens (supergroups already fetch it for the header).
- `Session::command_menu_items` lists every bot's commands in a basic group or
  megagroup (not broadcast channels). Bots whose user is not known yet are left
  out until it arrives, since the `@username` is needed. Basic groups put the bots that wrote last
  first, from the loaded history.
- `CommandMenuItem` carries its bot; rows show the userpic and
  `/command@bot`, and the filter matches `command@bot`.
- `mention_trigger` and `command_menu_trigger` take the caret and return the
  range to replace, so editing in the middle of a message works. `/` now needs
  to be the first character, as in tdesktop.
- Tab replaces the token before the caret and keeps the rest of the text; a
  click sends the command and keeps the rest.
- `insert_bot_command_text` follows `insertBotCommand`: prepends and replaces a
  leading command. Its panels are all in a bot's own chat, so it never adds
  `@bot`; clicked command links in group messages already add it in
  `send_bot_command` (`src/ui/entity_links.rs`).
- Bots in a group's `bot_commands` whose user is not cached are requested with
  `getUser` once each (`group_bot_users_tick`, driven from the poll loop, not
  render), and the menu fills in when they arrive.
- Mention queries follow tdesktop: up to 63 characters, first one a letter or
  digit.

## Not done

- Removing a recent inline bot from the `@` list (tdesktop
  `isRemovableMentionRow`): TDLib has no method for it (only
  `removeRecentHashtag`), so it is skipped.
- Enter on a menu row still inserts the command instead of sending it; that was
  already the case and is outside this gap.

## How verified

- Unit tests: triggers, caret handling, insertion, filtering, group rows
  (`src/composer/commands_tests.rs`), envelope parsing, reducer
  (`src/state/tests/group_bot_commands.rs`), driver fetch
  (`src/connect/tests/group_bot_commands.rs`).
- GPUI UI tests over the `ready-group-commands` demo
  (`src/ui/composer/group_commands_tests.rs`).
- Screenshot captures in light and dark of the `ready-group-commands` demo.
