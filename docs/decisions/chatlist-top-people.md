# Top-people strip in the empty search

Parity id: `chatlist-top-people`.

## What tdesktop does

- `dialogs/ui/top_peers_strip.cpp` draws a row of tiles (66 px wide, 46 px
  avatar, first name below) above Recent while the search field is focused and
  empty. Each tile has an unread badge (grey when muted), an online dot, and
  the Saved Messages avatar and name for your own chat (`TopPeersContent`,
  `dialogs/ui/dialogs_suggestions.cpp:2783`). Inaccessible users are skipped.
- The header reads "Frequent contacts". When the tiles do not fit one row
  (`count * single > width + single * 2 / 3`, `top_peers_strip.cpp:100`) it
  carries a link: "Show more" at first, then "Collapse" while expanded and
  "Show all" after a collapse (`:112-117`). Expanding grows the strip into a
  grid over `slideDuration` (240 ms) with `easeOutQuint` and spreads the tiles
  across the full width (`setExpanded`, `currentLayout`).
- The collapsed row scrolls with the wheel (a vertical wheel scrolls it
  sideways) and by dragging (`stripMouseMoveEvent`).
- Keys (`Suggestions::selectJumpChats`, `dialogs_suggestions.cpp:1685`): Down
  from the field enters the strip, Left and Right move across it, Up and Down
  move by rows when expanded, Down from a collapsed strip goes to the first
  recent chat, Up from the first recent chat returns to the strip.
- Click opens the chat, Ctrl-click opens it in a new window
  (`dialogs_widget.cpp:2170`), press and hold peeks at the chat
  (`handlePressForChatPreview`).
- Right-click menu (`FillEntryMenu`, `dialogs_suggestions.cpp:162`): Open in new
  window, Send Message, Add to folder (when the account has folders), View
  profile, then "Remove from Recent" (`TopPeers::remove`, which calls
  `contacts.resetTopPeerRating`) and "Remove all & Disable" with a confirmation
  (`TopPeers::toggleDisabled`).

## What Quill had

`getTopChats` and `removeTopChat` were wired, and the empty search showed a
plain strip of avatars with a button row for removal. It had no badges, no
online dot, no Saved Messages treatment, no expand link, no keys, no drag, and
the removal entries were not a menu.

## What changed

- `src/top_people.rs` (core) holds what the strip decides without a window:
  the people (`top_people`, first names, unread, muted, online, unknown chats
  skipped), the column and overflow rules, the three-state header link, the
  eased expand progress, the key walk (`step`) and the scroll that reveals a
  tile. All of it is unit tested.
- `src/ui/search_ui/top_people.rs` renders the strip and handles the keys. The
  expanded or collapsed choice lives in `SearchState::top_view` (reset when the
  search closes, like tdesktop recreating the widget), and the animation reads
  from the toggle time on the frame clock (`request_animation_tick`).
- The tile menu is `src/ui/message_actions/top_person_menu.rs`. It reuses the
  chat-row menu frame (now `chat_menu_shell`), the folder submenu and the
  profile entry, and adds the two removal entries. `ChatMenuState` gets a
  `top_person` flag; the old inline button row and `SearchState::top_menu` are
  gone.
- Down, Up, Left and Right in the search field step through the strip before
  the result list (`SearchRow::Top`, `chat_list_keys.rs`). Left and Right take
  the keys only while the strip shows, so the field keeps its caret once text
  is typed.
- `getTopChats` asks for 30, the most TDLib allows (tdesktop keeps 64 from its
  own list).

## Deviations

- The unread badge uses the kit badge, which caps at "99+" like Quill's chat
  rows; tdesktop's strip prints "1K" above 999.
- The strip width is estimated from the sidebar width minus the results
  padding, not measured.

## How verified

`cargo test --no-default-features top_people` (rules, key walk, session
mapping) and the UI tests in `src/ui/search_ui/top_people_tests.rs` (keys,
Enter, click, header link, menu removal, disable confirmation). Captures from
the `ready-top-people` demo in light and dark: collapsed row with badges,
expanded grid, tile menu.
