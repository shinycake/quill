# Keyboard shortcut pack (B1)

## What tdesktop does

`core/shortcuts.cpp` `fillDefaults` registers, with Qt's `ctrl` (which Qt
maps to Cmd on macOS):

- `ctrl+1..8` open pinned chats 1-8 of the list on screen (only when that
  many pinned chats exist), `ctrl+0` Saved Messages, `ctrl+9` the Archive
  (only when it is not empty), `ctrl+j` Contacts;
- `ctrl+pgdown/pgup` next/previous chat (besides `alt+down/up`),
  `ctrl+alt+home/end` first/last chat;
- `ctrl+shift+down/up` next/previous folder, written with `meta`, so a
  real Ctrl on macOS and Ctrl elsewhere. It does not wrap
  (`CheckAndJumpToNearChatsFilter`);
- `ctrl+r` read the chat (only if unread), `ctrl+\` chat menu, `ctrl+]`
  chat preview. (`ctrl+r` is also registered for RecordVoice, but the
  first registration of a sequence wins.) tdesktop acts on the hovered or
  selected dialog row; Quill has no keyboard row selection, so it acts on
  the open chat.

`HistoryWidget::keyPressEvent` and `eventFilter`: Ctrl+Up / Down reply to
the previous / next message (`replyToPreviousMessage` /
`replyToNextMessage`): they skip local messages, forums and edit mode;
Up with no reply starts at the newest message; Down past the newest
cancels the reply; the message is also scrolled into view. On macOS,
Cmd+Up / Down are the field's Home / End, so the reply only happens when
the field has no text. Ctrl+O opens the attach dialog (`chooseAttach`).

`HistoryInner::keyPressEvent`: Delete / Backspace with a selection opens
the delete box when every selected message can be deleted. Page keys
scroll the history; the main field is a `_customUpDown` field that ignores
Up/Down/PageUp/PageDown so they reach the history, while Home/End stay
with the field. Esc clears the selection first (`HistoryWidget::escape`).

## What changed in Quill

- `src/ui/shortcut_pack.rs`: handlers plus pure helpers (`reply_nav`,
  `page_up_target`, `page_down_target`, `near_folder`, `visible_pinned`).
  Each handler returns whether it used the key; otherwise the action
  listener propagates the keystroke to the focused input.
- `src/ui/keybindings.rs`: new rows (chat navigation, pinned chats,
  messages) show up in the Help > Keyboard Shortcuts dialog. They are not
  user-rebindable; they claim their chords like window chrome, so a
  rebindable action cannot be moved onto one. Cmd/Ctrl+PageUp/PageDown
  join Next/Previous chat as extra default chords.
- Composer twins: the kit's `Input` context binds PageUp/PageDown, and on
  macOS Cmd+Up/Down and Cmd+], so the same actions are also bound in
  `QuillComposer > Input`. When the composer should keep the key (a
  multi-line draft for the page keys; a non-empty draft on macOS for
  Cmd+Up/Down), the handler propagates and the kit binding runs.
  Home/End are not stolen from the composer.
- Conflicts resolved: Cmd/Ctrl+1 was "Focus chat list" and Cmd/Ctrl+Up was
  "Load older messages". Focus chat list moved to Cmd/Ctrl+Alt+1 and
  Load older to Alt+PageUp (saved custom chords for those two actions are
  unaffected). Ctrl+L stays "Focus composer".
- Esc: selection mode is cleared before a pending reply or edit header.
- Delete / Backspace: opens the existing multi-message delete confirmation
  (`confirm_delete_selection`) when messages are selected, unless the
  composer holds text.
- History scrolling: the conversation paint records the first and last
  visible rows (`scroll_view_probe`). PageUp/PageDown move by that many
  rows, with one row overlapping; Home goes to the oldest loaded row and
  asks for older history; End is the jump-to-latest button's action. Scrolling
  is row granular (the kit `MessageScrollerState` has no pixel scroll
  API).
- Shortcuts do nothing while the passcode lock, a dialog, the media viewer
  or the story viewer is up.

## Not done

- Middle-click autoscroll: the kit scroller exposes only row targets, so
  continuous velocity scrolling is not feasible without forking it.
- Ctrl+1..8 in a folder use the pinned chats that are also visible in that
  folder (Quill does not track `is_pinned` per folder).
- The Appearance rebinding list was not extended with the pack; the
  reference dialog was.

## Verification

Unit tests: reply navigation, page targets, folder stepping, pinned order
(`shortcut_pack.rs`); chord to action resolution for every pack chord,
composer-context precedence over the kit input bindings, and that pack
chords cannot be rebound onto (`keybindings.rs`). The gate (fmt, clippy,
core and UI tests) passes. Not exercised in a live window or a live
account.
