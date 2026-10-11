## Open a chat in a new window — slice 3: notifications, own fields, passcode, saved windows (2026-10-11)

Follow-up to `codex-chat-new-window.md` and `-2.md`. Four small gaps the
second slice listed, each checked against Telegram Desktop first.

### Notifications for a chat shown in an active window

- **tdesktop:** `History::newItemAdded` schedules a notification only
  while `item->showNotification()` holds, and a message the active window
  shows is read on arrival (`HistoryWidget` reads what is visible while
  its window is active), so the chat in front never notifies, in whichever
  window it is.
- **Quill:** `Session::active_window_chats` lists the window chats whose
  window is the active one; each chat window's render writes it
  (`QuillApp::set_window_chat_active`, like `app_active` for the main
  window), a closing window clears it. `NotifyInput::shown_in_active_window`
  (`notify::decide_notify`) suppresses the toast and sound the way
  `app_active && open_chat == chat` does for the main window.
- Test: `state/tests/window_chats.rs` — a message for a chat in an active
  window queues nothing, the same chat once its window is inactive queues
  one; the main window rule unchanged; `Session::swap_view` symmetric.

### Chats open in a window are not marked in the chat list

- **tdesktop:** `dialogs_inner_widget.cpp` highlights the window's own
  active chat and nothing about other windows; skipped as briefed.

### Per-window search-in-chat and share fields

- The "Search in chat" field (`SearchUi::chat_input`) and the share box
  (`ShareUi`: forward selection, destination picker and its field,
  "Reply in Another Chat", the attach menu's Contact / Location panel) are
  part of a window's slot now and swap with the rest of it. Their wiring
  moved out of `new_with_demo` into `ui/search_fields.rs`
  (`new_chat_search_field`, `new_forward_search_field`), used by the main
  window and every chat window.
- Test: `ui/chat_window/tests.rs` — with the window's state in place the
  fields are other entities than the main window's; searching in the
  window (header search button, typing) fills the window's field and opens
  the window's chat search while the main window's field stays empty and
  its search closed.

### The passcode box in every window

- **tdesktop:** `Application::lockByPasscode` runs `setupPasscodeLock` on
  every window (`window_controller.cpp`), so each shows the box.
- **Quill:** a locked chat window shows the same `lock_overlay` as the main
  window (one passcode, one box state, unlocking anywhere unlocks all) and
  runs `passcode_frame` for the field focus and the shake, instead of the
  slice-1 cover that pointed to the main window.
- Test: locking shows `lock-submit` in the chat window and hides the
  composer; unlocking brings the composer back.

### Chat windows across launches

- **tdesktop:** `Window::SavedWindows` saves the separate windows with
  their positions on every change (`scheduleSave`), asks once on a later
  launch whether to restore them (`lng_restore_windows_*`, `kAskedKey` /
  `kRestoreKey`) and restores them once the account is ready and unlocked
  (`startRestore`); windows whose chat cannot be resolved are dropped.
- **Quill:** `settings/chat_windows.rs` keeps `chat_windows.json`
  (`asked`, `restore`, the windows per account with geometry).
  `ui/chat_window/saved.rs` writes the open windows 600 ms after one
  opens, closes, moves or resizes (live sessions only; demos and tests
  never touch the file). From the main window's frame upkeep, once the
  account is ready: with saved windows and no answer yet, an alert asks
  "Restore chat windows?" (Restore / Not now; the answer is kept); with a
  yes, the windows open at their saved geometry as their chats load, and
  what is left when the chat list is complete is dropped. The reopen path
  is `open_chat_window_at(chat, Some(bounds))`.
- Tests: `settings/chat_windows.rs` (per-account replace, sanitizing,
  file round trip, broken file), `ui/chat_window/saved.rs`
  (`to_restore` follows the answer and the account, `split_ready` opens
  loaded chats and keeps the rest waiting).
- Not done: tdesktop's "reopen last closed window" shortcut and the
  restore-shell placeholder it shows while a chat resolves.

### Verified

- Gate (fmt, core + UI clippy `-D warnings`, core + UI tests),
  `cargo test --features demo-capture --bin quill chat_window` (11),
  `check-hotspots.sh origin/main` (`Hotspot-change:` for `app_demo.rs`,
  the search fields' wiring moved out), `check-file-size.sh`.
