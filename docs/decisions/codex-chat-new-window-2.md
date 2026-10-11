## Open a chat in a new window — slice 2: a self-sufficient window (2026-10-11)

- **Gap:** after slice 1 (`codex-chat-new-window.md`) a chat window showed
  the chat and sent text, but everything else it asked for happened in the
  main window: the message menu, the reaction picker, the media viewer and
  kit dialogs opened there; the window's compose field had no replies,
  edits, attachments, pickers or typing indicator; drag selection and GIF
  autoplay stayed with the main window; forums were not offered.
  Telegram Desktop's separate window is a full `SessionController`: its
  own active chat, menus, boxes, compose field and pickers
  (`window/window_controller.cpp`, `window/window_session_controller.cpp`),
  and `showInNewWindow` opens forums as a `Forum` window with the topic
  list (`window/window_peer_menu.cpp` `Filler::addNewWindow`).

### Design: one slot per window, swapped in while the window draws or handles input

Quill keeps one `QuillApp` and one `Session`. Their view state is written
by hundreds of handlers (`self.history`, `self.composer`,
`self.composer_ui`, `self.message_ui`, `self.pickers`, `self.viewer`, the
session's `open_chat`, `open_topic`, thread, Saved Messages and search
views, the gallery). Rather than thread a window through all of them, a
chat window owns a copy of each in a `WindowSlot`
(`ui/chat_window/slot.rs`) and `QuillApp::set_window_scope(target)`
exchanges the slot with the app's fields (and `Session::swap_view` with
the session's `SessionView`, `state/session_window_chats.rs`). While a
window's slot is in place, every existing path — `conversation(Top)`,
`conversation(Bottom)`, `submit_composer`, `open_message_menu`,
`toggle_media_panel`, `open_media_viewer`, the drafts, the typing
indicator, the dialog builders — reads and writes that window's state,
because `open_chat` *is* the window's chat then.

When the slot is in place:

- **Draw:** `ChatWindowView::render` puts its slot in place, builds the body
  and restores what was there; `WindowScope` does the same around the
  body's layout and paint (the message scroller builds rows lazily during
  prepaint). The main window's render and root do the same with the main
  state (`WindowScope::new(None)`), so a window drawing in the middle of
  another window's input never leaves the wrong state behind.
- **Input:** a pointer event's capture phase runs the window root's
  `scope_probe` (window-level listeners registered in paint, before any
  element handler); a key event runs the keystroke interceptor
  `ChatWindows::new` registers, before the kit's input handler and the
  app's own interceptors. Both call `note_window_input(chat)`, which also
  records the window for dialogs.
- **Deferred and async work** the window's rendering schedules carries its
  window and runs under `in_window_scope`: older/newer paging, the jump
  button, GIF autoplay, the file picker's answer.
- **Leaving:** a window's state stays in place after its input until the
  next main-window input, draw or poll (`poll_live` resets it, so TDLib
  updates apply to the main view); `chat_window_closed` resets it before
  dropping the slot.

What stays shared on purpose: the chat list, folders, the info column,
downloads, playback (one player), recording (one microphone), calls,
stories, settings, the search inputs' entities, the forward/share state.

- **Dialogs:** kit dialogs follow app-wide flags; `QuillShell::sync_kit_dialogs_in(host)`
  opens a dialog in the window the last input happened in
  (`ChatWindows::last_input`) and keeps it there (`dialog_host`) until
  it closes; each chat window owns a `QuillShell` for its sync and the
  hit-test barrier. The builders run with the host window's slot in place.
- **Composer:** `new_composer_field` / `subscribe_composer_events`
  (`ui/composer_events.rs`, moved out of `new_with_demo`) wire a window's
  field like the main one's: replacements, typing, drafts, the `/`, `@`,
  `#`, `:emoji` menus, inline mode, spell check, sticker suggestions,
  Enter. Replies, edits, attachments (drop, paste, the picker), the
  emoji/sticker/GIF panel and the reaction selector come from the slot's
  `composer_ui` and `pickers`.
- **Navigation:** a chat picked from inside a chat window (link, mention,
  search hit) opens in the main window and activates it
  (`routed_to_main_window`; tdesktop hands a non-primary window's
  navigation to the primary one); the window keeps its chat.
- **Frame clock:** `frame.window_active` is true while any Quill window is
  active (`ChatWindows::any_active`), so animations run in an active chat
  window with the main window behind.
- **Forums:** offered now; the window lays out the topic column beside the
  history (`forum_column_layout` / `forum_column_view`, as the main window
  does where the chat list was), topics only on a narrow window until one
  is picked.
- **Driver:** `open_chat_window` runs the same follow-ups as `select_chat`
  (`fetch_open_chat_extras`: membership, bot info, keyboard, thumbnails,
  sponsored rows, forum flag and topics, header extras) under
  `with_window_chat`, which makes the window's chat the open one for the
  call and restores the main view.

### Verified

- UI integration tests (`ui/chat_window/tests.rs`, `cargo test --features
  demo-capture --bin quill chat_window`): typing + Enter in the window
  sends to the window's chat through the shared composer path, main chat
  and composer untouched; a right-click in the window opens the message
  menu in the window's state only, Reply arms the window's field and the
  sent message carries the reply; the emoji button opens the window's
  picker, not the main one's; a dialog asked for after input in the
  window opens in that window's shell only, and after it closes the next
  one asked for from the main window opens there; a chat picked inside the
  window opens in the main window while the window keeps its chat; the
  slice-1 cases (moving the active chat, reuse, Cmd/Ctrl+W, Ctrl/Cmd+click).
- Driver tests (`connect/tests/chat_window.rs`): the slice-1 set plus the
  follow-ups running for the window chat with the main view (open chat,
  topic) untouched, and `with_window_chat` restoring it.
- Demo capture (temporary hook, not committed): the window for chat B with
  the full composer (attach, emoji, format, mic), header search and ⋮
  menu, no info toggle; the main window unchanged.
- Gate (fmt, core + UI clippy `-D warnings`, core + UI tests),
  `check-hotspots.sh origin/main` (`Hotspot-change:` for `app_demo.rs`,
  the composer wiring moved out, and `shell.rs`, the hosted dialog sync),
  `check-file-size.sh`.

### Not done / reported

- The passcode box does not show in a chat window (a cover points to the
  main window; tdesktop locks every window).
- The info column, Saved Messages sublists navigation from the main
  window's panels, downloads and shared-media panels stay in the main
  window.
- Notifications for a chat window's chat are suppressed only through the
  main window's `app_active`/`open_chat`; a message arriving in an active
  chat window still notifies.
- Window positions are not saved across launches; the sidebar does not
  mark chats that are open in a window.
- Search and forward pickers share their text-field entities across
  windows (one search bar at a time works; two open at once would share
  the text).
