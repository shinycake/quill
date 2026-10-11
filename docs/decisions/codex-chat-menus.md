# Chat row and header menus (codex-chat-menus)

## What tdesktop does

`window/window_peer_menu.cpp`, `Filler`:

- History (header) menu, `fillHistoryActions`: mute, new topic, info, view as
  topics, manage, boost, create poll, set wallpaper (`addThemeEdit`, private
  chats with another user), discussion, export, report. The profile section
  (`fillProfileActions`) adds auto-delete, gift, statistics.
- Conditions: boost for supergroups and channels; polls by `canCreatePolls`
  (own chat and bots for users, never secret chats, send permission in
  groups); statistics only with `CanGetStatistics`; report unless you created
  the chat.
- Report opens the report-reason box: `reportChat` with the server's
  option lists (`reportChatResult`), then optional text.
- Mark mentions, reactions and poll votes read are `SetupUnreadMentionsMenu`
  and siblings: right-click menus on the corner buttons.

## What changed

- `src/chat_header_menu.rs`: pure entry list and conditions for the header
  menu, in tdesktop order, with unit tests per chat kind and permissions.
- `src/ui/chat_header_menu_ui.rs`: the header menu builder moved out of
  `navigation.rs`; new entries open existing flows (info panel, boost panel,
  statistics panel, poll dialog, chat look dialog, report flow).
- Chat row "Report" (and the header's) now starts the reason flow used for
  messages, with no message ids, instead of a one-click spam report. The
  dialog says "chat" instead of "message" then. `GroupConfirmAction::ReportChat`
  is gone.
- `chatlist_menu::profile_label` is shared by both menus.
- The row menu already had view profile, new window, export, and the three
  mark-read entries (`row_menu_extras`); `readAllChatMentions`,
  `readAllChatReactions` and `readAllChatPollVotes` JSON is now tested.

## Verification

`gate.sh` (core and ui tests), hotspot and file-size checks. Not checked
visually and not run against a live account: reporting a real chat was
deliberately avoided.
