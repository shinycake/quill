# Caches keyed with their owner

Several UI caches were keyed by an id that is only unique inside an owner.
TDLib message ids repeat between chats in supergroups and channels, and chat
ids are the same for a group two signed-in accounts share.

tdesktop keeps this state per history (`HistoryItem` lives in its `History`,
and each session has its own `Data::Session`), so the key never needed the
owner. Quill keeps it app-wide, so the owner is in the key now.

Changed:

- `playback.positions` and `playback.gif_decodes`: `ChatMessage { chat, message }`.
  A GIF decode that finishes (or shows its first picture) only starts playing
  when its chat is the open one.
- `vanish::PAINTED`: `ChatMessage`. Skipped: the window half of the key. A chat
  has one window, so the chat already tells windows apart.
- `send_fly::CELLS`: `CellKey { window, file }`. The cell is read for the window
  the last input happened in (`windows.last_input`). The key is the sticker
  file id, not a cell index.
- `premium_sticker_effect::Played`: remembers its account and starts over when
  another account is on screen.
- `history.pinned_cursor` and `hidden_pinned`: `AccountChat`.
- `earn.photos_asked`, `ai_compose.emoji_asked`, `mini_apps.emoji_access_shown`:
  `OfAccount<i64>`.

Verified with unit tests: a saved position stays with its chat
(`a_saved_position_belongs_to_its_chat`) and the played set resets per account
(`another_account_starts_over_in_the_same_chat`). Not live-checked.
