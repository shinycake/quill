## Open a chat in a new window — first slice (2026-10-11)

- **Gap:** `parity:platform-new-window`. Quill had one window; the chat row
  menu and the header menu had no "Open in new window", and Ctrl/Cmd+click
  on a row selected the chat like a plain click.

### Spec (Telegram Desktop, read before building)

- `window/window_separate_id.h`: a window is identified by a `SeparateId`
  (`Primary`, `Archive`, `Chat`, `Forum`, `Community`, `SavedSublist`,
  `SharedMedia`) plus the account and, for the chat kinds, the thread.
  `hasChatsList()` is true only for Primary, Archive, Forum and Community:
  a `Chat` window is header + history + compose field, no chat list.
- `core/application.cpp`: `ensureSeparateWindowFor(id)` activates an
  existing window for that id, else creates a `Window::Controller`,
  installs the passcode lock when locked, shows and activates it.
  `windowForShowingHistory(peer)` prefers the peer's separate window.
  `closeChatFromWindows(peer)` closes the peer's windows when the chat
  goes away.
- `window/window_session_controller.cpp` `showInNewWindow`: when the
  chat is the active one, the main window first clears its section stack
  (shows no chat), then the separate window opens; when it cannot be shown
  (`SeparateWindowLocked`: passcode or terms lock; unavailable peer) the
  chat opens in place instead.
- `mainwidget.cpp` `showHistoryInDifferentWindow`: picking a chat that has
  a separate window shows and activates that window, the current window
  does not switch. The same routing applies to notification clicks
  (`notifications_manager.cpp` → `ensureSeparateWindowFor`).
- `window/window_peer_menu.cpp` `Filler::addNewWindow`: the entry heads the
  chat-list row menu and sits in the history (⋮) menu; forums open as a
  `Forum` window (with the topic list), communities as a `Community`
  window, the archive folder as an `Archive` window.
- `dialogs/dialogs_inner_widget.cpp`: `row.newWindow = modifiers &
  Qt::ControlModifier` (Cmd on macOS) opens the clicked row in a new
  window.
- `history/history_widget.cpp` `closeCurrent`: in a non-primary window
  "close" closes the window. Drafts, read state and the history itself are
  shared data; the compose field and scroll position belong to the window.
- `window/main_window.cpp`: child windows are 3/4 of the default size and
  cascade from the parent; `updateTitle` runs for every window, so a chat
  window's title is the chat name with its unread count and the total.

### Design: shared vs per window

Quill has one `QuillApp` entity holding one `Session` and one TDLib
client; Telegram Desktop has one `SessionController` per window. Rather
than split `QuillApp` (its history renderer reads "the open chat" in
dozens of places, and every overlay, dialog and playback state lives on
it), the slice keeps `QuillApp` as the shared app model and adds a small
per-window view:

- **Shared** (`QuillApp`, one per process): `Session` + `ConnectDriver`
  (one TDLib client), the poll loop, notifications, playback, dialogs,
  preferences, the main window's `open_chat`, composer and `HistoryUi`.
- **Per window** (`ui/chat_window/view.rs`, `ChatWindowView`): the chat it
  shows, its `HistoryUi` (scroller, row cache, visibility probes, pinned
  cursor…), its compose field (`TextareaState` with the same span styler,
  send-key mode and draft text), the last send error, the native title.
- **The swap** (`WindowScope`): the window's body is built by `QuillApp`'s
  own `conversation_header` and `history_message_list`. While a chat window
  is built, laid out or painted, its `HistoryUi` is swapped into
  `QuillApp::history` and `ChatWindows::scope` names its chat;
  `QuillApp::viewed_chat()` / `viewed_topic()` resolve to it, and the
  history path (`history_message_list`, `render_history_row`,
  `report_visible_history`, the row list's chat look) reads those instead
  of `Session::open_chat`. The element wrapper enters and leaves the scope
  around each phase without holding the app lease, so the message
  scroller's lazy row renderer (which updates `QuillApp` during prepaint)
  works unchanged. Deferred callbacks run outside the scope, so the ones the
  list schedules (older / newer paging, the jump button) carry the window's
  chat explicitly (`maybe_auto_load_older_in`, `jump_to_latest_in_window`);
  GIF autoplay and drag selection stay with the main window for now.
- **Session side** (`state/session_window_chats.rs`,
  `connect/chat_window.rs`): `Session::window_chats` lists the chats shown
  in their own windows; `chat_is_shown(chat)` is "open chat or window
  chat". `open_chat_window` sends `openChat` (unless the main window has
  the chat open: `openChat` is a per-chat flag in TDLib), prepares the
  history view (`prepare_history_view`, extracted from `Session::open_chat`)
  and loads the first page; `close_chat_window` sends `closeChat` unless the
  main window still shows the chat; `select_chat` skips `openChat` for a
  window chat and `close_open_chat` skips `closeChat` for one.
  `fetch_history` / `fetch_history_newer` / `retry_history` /
  `maybe_view_open_messages` got `_for(chat_id)` forms gated on
  `chat_is_shown`. History pages of a window chat are not stale when the
  main window's `view_generation` moves on (`history_view_stale`), and
  `report_visible_messages` accepts window chats, so the window reads its
  rows. `deselect_chat` + `Session::leave_open_chat` give the main window
  an empty pane (tdesktop's `clearSectionStack`).
- **Routing** (`ui/chat_window/mod.rs`): `open_chat_in_new_window`
  activates an existing window or opens one (3/4 of the main window,
  cascaded), moving the active chat out of the main window first
  (`leave_open_chat`: draft flushed, composer cleared, per-chat composer
  state dropped, info panel closed). `select_chat_with` and the notification
  click activate the chat's window instead of switching the main window.
  The window closes itself when its chat or the session goes away, and
  shows a lock cover while the passcode is on. Cmd/Ctrl+W closes it.
- **Entry points:** chat row Ctrl/Cmd+click (`chat_row`), the row menu's
  first entry (`chat_menu_overlay`), the header ⋮ menu
  (`NavigationAction::NewWindow`). Offered for supported non-forum chats
  while unlocked (`can_open_chat_window`).
- `HistoryUi::new` takes `&mut App` so a window can build one; the
  `rows_input` closure in `session_history` became
  `history_rows_input(list, chat, topic, history, highlight)` so the window
  shares the row cache key. Demo sends (`apply_demo_outgoing_to`) now
  advance the demo sequence past the seed's `last_seq`; before, the first
  demo sends after seeding were dropped as out of order.

### Verified

- Driver tests (`connect/tests/chat_window.rs`): a window chat opens and
  pages beside the main chat (`openChat`, `getChatHistory` per chat,
  per-chat in-flight dedupe); its pages land after the main window
  switched chats; the main window switching away keeps a window chat open
  and the window's close sends the paired `closeChat`; closing the window
  while the main window shows the chat defers the close to the main
  window's switch; moving the active chat to a window leaves the main
  window empty without `closeChat`; window rows are viewed
  (`messageSourceChatHistory`) and sends go to the window's chat; a chat
  that is not shown cannot page.
- UI integration tests (`ui/chat_window/tests.rs`, `#[gpui_kit::test]`,
  `cargo test --features demo-capture --bin quill chat_window`): opening a
  window for another chat keeps the main chat, typing in the window's
  field and pressing Enter sends to the window's chat (the main chat and
  composer untouched, the field cleared); opening the active chat moves it
  out of the main window and picking it in the list activates the window
  while other chats still open in the main window; asking again reuses the
  window and Cmd/Ctrl+W closes it and frees the chat; Ctrl/Cmd+click on a
  chat row opens its window.
- Demo capture (temporary hook, not committed): `ready-chats` with a window
  for chat B — title "Demo chat B – (1)", header, history, compose field
  with Send; the main window unchanged on chat A.
- Gate (fmt, core + UI clippy `-D warnings`, core + UI tests),
  `check-hotspots.sh origin/main`, `check-file-size.sh`.

### Not done / reported

- Parity not ticked: the window is a plain chat window. Left for the next
  slices: message menus, reaction pickers, the media viewer and kit dialogs
  opened from a chat window still appear in the main window (they are
  `QuillApp` overlays keyed to it); the window's compose field sends text
  only (no attachments, replies, edits, stickers, typing indicator);
  forums (tdesktop's `Forum` window with the topic list), communities and
  the archive folder are not offered; the passcode box does not show in the
  window (a cover asks for the main window); drag selection and GIF
  autoplay stay with the main window; the frame clock follows the main
  window's active state, so animated emoji in a background-main / active
  chat window pause.
- Window positions are not saved across launches (tdesktop's
  `_savedWindows`); the sidebar's active-row highlight does not mark chats
  that are open in a window.
