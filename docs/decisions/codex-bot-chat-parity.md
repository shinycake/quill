# Bot chats: conversation first, commands behind a Menu button (tdesktop)

Found live: a bot chat showed a permanent panel above the history with
the bot's description, every command as a button and the profile
actions. The Hermesio bot's long command list covered the whole chat.

Telegram Desktop:
- shows the bot card ("What can this bot do?", description, START) only
  in an empty chat;
- once there are messages it's a normal conversation;
- commands sit behind a "Menu" button left of the composer (the bot's web
  menu when it has one), or appear when typing "/", in a compact
  scrolling popup.

Decision:
- `bot_info_panel` renders only while the chat is empty or a START deep
  link is pending.
- A `bot_menu_button` (Menu icon, the bot's menu text when set) sits
  left of the composer: it opens the web menu URL, else types "/" to show
  the command list.
- The command list is capped at 320 px and scrolls.

Profile actions (restart, share, block, similar bots) belong in the bot's
profile, where tdesktop keeps them; they are reachable in the empty-chat
card for now.

Verified live in a bot chat.
