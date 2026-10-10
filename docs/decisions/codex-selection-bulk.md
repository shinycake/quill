# Bulk selection: keyboard, drag, Download, scheduled messages, protected-chat note

Follow-up to codex-selection-pin. That PR gave the selection bar Copy as
Text, Save, Unpin and Report, plus Shift+click ranges and dragging once a
message was selected.

## What tdesktop does

- `HistoryInner` lets you press a message and drag over others to select
  them, but only when the press did not start on text (a press on text
  starts a text selection). The menu also offers "Select up to this
  message" (`selectItemsUpTo`), which fills the gap to the nearest selected
  message and stops at `MaxSelectedItems` (100).
- Ctrl+Space (Cmd on macOS) toggles the focused message and starts
  selection mode. While something is selected, Space keeps toggling, Up and
  Down move the focus, and Shift+Up/Down grow or shrink the range. The
  focus is the accessibility focus, so it matters mostly for screen
  readers.
- The selection top bar shows Forward and Delete, plus Send now in the
  scheduled section (`ConfirmSendNowSelectedItems`, "Send N messages
  now?"). Delete asks `lng_selected_delete_sure*`.
- A message menu with nothing to copy or forward ends with one line saying
  why (`AddSelectRestrictionAction`): channel, group, bot, "You disabled..."
  or "{user} disabled...".
- The pinned bar's close button unpins, with the unpin question, when you
  can pin. Otherwise it hides the bar, with the hide question.

## What changed

- `selection_pin.rs` has the new pure rules, all unit tested: `up_to`,
  `room_for` with `MAX_SELECTED`, `step_focus`, `range_delta`, the list
  helpers `toggle_id` and `prune_selected`, and the send-now and delete
  questions. The pinned-bar questions moved there as constants so a test
  pins their wording.
- Keyboard: Cmd/Ctrl+Space, Up, Down, Shift+Up and Shift+Down
  (`ToggleMessageSelection`, `SelectionFocus*`, `SelectionExtend*`), listed
  in the shortcuts dialog. Up and Down only act while a selection is open in
  the chat and no composer text would lose its caret keys; otherwise they
  propagate. The focused row gets a ring and the history scrolls to it.
  Unlike tdesktop, arrow focus needs selection mode. Cmd/Ctrl+Space starts
  it on the focused row, or on the newest message when none is focused.
  Plain Space stays unbound because the composer and story viewer use it.
  On macOS, Cmd+Space is Spotlight's default, so the chord works only after
  that shortcut is changed.
- "Select up to this message" in the message menu while a selection is open.
- Drag from a row when nothing is selected: a press that did not start on
  text, then a move onto another row, starts selection and gives the rows
  under the pointer the drag's state. Selection stops at 100 messages.
- "Download" in the selection bar when selected media is not on disk. It
  starts the downloads, and Save then copies the files out.
- Scheduled messages dialog: each row has a checkbox. With any ticked, a bar
  shows "N selected" with Send now, Reschedule, Delete and Clear. Send now
  and Delete confirm with tdesktop's questions. Reschedule opens the same
  date and time picker and applies the time to every ticked message. The
  per-row buttons stay. The dialog does not share the history selection
  code, because scheduled messages live in `Session::scheduled_messages`
  and not in a history, so it uses the shared pure helpers with its own
  small list state.
- Protected-chat line: bot chats say "Copying and forwarding is not allowed
  from this bot.", and a private chat the other side restricted names them
  ("Anna disabled copying and forwarding in this chat."). TDLib does not say
  who turned the setting on, so an outgoing message still reads "You
  disabled...".
- Pinned bar: the questions already matched tdesktop and are now shared
  constants with a test. tdesktop's bar has no "Pin" action, so none was
  added.

## Not done

- tdesktop bolds the user's name in the protected-chat line.
- Space to toggle while selecting, and screen-reader focus outside
  selection mode.
- Bulk Reschedule has no tdesktop counterpart (its scheduled bar has Send
  now and Delete); it is an extra.

## Verified

- Gate: fmt, clippy, core and UI tests (`GATE OK`).
- Unit tests for each pure rule, the new chords in the keybinding table,
  and the protected-chat wording.
- Demo captures with English fixtures, looked at: `ready-select-keyboard`
  (focus ring, Download and Save), `ready-scheduled` with
  `QUILL_DEMO_SCHEDULED=list-selected`, and `ready-message-menu` with
  `QUILL_DEMO_MENU=protected`.
- Not exercised interactively: the drag from outside selection mode and the
  keyboard handlers. A demo capture is a static frame and no account was
  driven, so the parity fragment leaves `selection-drag-range` and
  `selection-keyboard` unclaimed.
