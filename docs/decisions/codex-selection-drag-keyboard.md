# Message selection: drag across rows, Shift+click ranges, keyboard

Finishes `parity:selection-drag-range` and `parity:selection-keyboard`,
which codex-selection-bulk left unclaimed because the drag and the keys
were never exercised.

## What tdesktop does

Read from `history/history_inner_widget.cpp` (`mouseActionStart`,
`mouseActionUpdate`, `mouseActionFinish`, `applyDragSelection`,
`keyPressEvent`), `history/view/history_view_list_widget.cpp`
(`ensureDragSelectAction`) and `lib_ui/ui/dragging_scroll_manager.cpp`.

- A press on text starts a text selection. A press anywhere else on a row
  is `PrepareSelect`; once the pointer has moved `startDragDistance` it is
  `Selecting`, and the rows between the press row and the row under the
  pointer form the contiguous range `_dragSelFrom.._dragSelTo` (swapped
  into screen order). The range previews while the button is down: in
  select mode its rows paint as selected, in deselect mode as plain. The
  range applies on release.
- Select or deselect is decided once per drag, by the first affected row:
  `dragSelecting = !_selected.contains(dragFirstAffected)`, so a drag that
  starts on a selected row deselects everything it covers.
- With messages selected, a press and release without a drag toggles the
  row at release (`needItemSelectionToggle`); links and media don't fire.
  Albums toggle as a group.
- The selection stops at `MaxSelectedItems` (100). tdesktop has no
  Shift+click range for messages; the nearest feature is the menu's
  "Select up to this message", which fills from the nearest selected
  message and stops at the limit.
- Edge autoscroll (`DraggingScrollManager`): while selecting, every 15 ms
  the list scrolls `min(delta * 3 / 20 + 1, 37)` px, where `delta` is how
  far the pointer is beyond the viewport's top or bottom edge.
- Keyboard (`keyPressEvent`, only in screen-reader mode there): Ctrl+Space
  toggles the focused message and enters selection mode; while anything is
  selected, plain Space keeps toggling; Up / Down move the focus and
  scroll only when the row is off screen; Shift+Up / Down grow or shrink
  the range from an anchor, and a mouse action re-anchors; Esc clears;
  Delete and Backspace delete the selection; drag-selecting gives the
  history focus (`setFocus`), and keys the history doesn't use go to the
  composer (`tryProcessKeyInput`).

## What Quill had

Clicking toggled at press, a drag toggled rows one by one as the pointer
crossed them (a fast drag skipped rows, moving back never reverted, no
preview, no autoscroll), the overlay occluded the wheel so a selected row
could not be scrolled over, keyboard focus moves ran a chat-search jump,
and Space did nothing.

## What changed

- `src/selection_drag.rs` (core, no GPUI): `DragRange` (anchor, pointer
  row, select or deselect; `covers`, `shown`), `apply_drag` (adds nearest
  the anchor first until the limit, or removes the covered rows),
  `moved_enough` (the 10 px start distance), `row_under` (hit testing
  against the painted rows, clamping to the edge rows outside them),
  `edge_delta` and `autoscroll_step` (tdesktop's numbers). Unit tested.
  One deliberate difference: past 100 messages tdesktop keeps the top of
  the range; Quill keeps the part next to where the drag started.
- `src/ui/selection_drag.rs`: the press, pointer and release handlers.
  The row's left press (outside selection mode) and the selection
  overlay's press (inside it) both call `selection_press_row`; a window
  level pointer listener registered from the history list's probe canvas
  feeds `selection_drag_pointer`, and a window-level release calls
  `selection_release`. Element listeners could not do this: the overlay
  blocks hover, so the list's own `on_mouse_move` never sees a drag over
  a selected row. While a button is held a 15 ms task polls
  `window.mouse_position()`, so a pointer resting beyond the edge keeps
  scrolling and extending the range; it scrolls through the same wheel
  event path as middle-click autoscroll (the kit scroller has no pixel
  offset API). The loop ends with the drag.
- The history list keeps each painted row's bounds (`HistoryUi::hit_rows`)
  and its own bounds (`viewport`) from the floating-date probe paint, so
  hit testing is against what was actually drawn.
- Rows preview the drag: `selection_shown` gives a covered row the drag's
  state, `selection_visible_in` turns the mode on for a drag that starts
  outside it, and the bar's count follows the preview.
- The overlay uses `block_mouse_except_scroll` instead of `occlude`, so the
  wheel reaches the list again over selected rows.
- Shift+click selects the range from the last clicked row (anchor) to the
  clicked one, only ever adding; a click or a drag re-anchors.
- Keyboard: Ctrl+Space joins Cmd/Ctrl+Shift+A for `ToggleMessageSelection`
  (some desktops take Ctrl+Space as an input-source switch, so the Shift+A
  chord stays). A row press, a drag or the toggle chord move focus to the
  history (`MessageUi::history_focus`, tracked by the list). With that
  focus, Space toggles the focused message and typed characters go to the
  composer, as tdesktop forwards them, so a selection never swallows
  typing. Up / Down scroll the focused row into view only when it is off
  screen, instead of running a search jump.
- `selection_mode.rs` lost its per-row drag code (now 884 lines).

## Verified

- Core unit tests for the model (range direction, preview, select and
  deselect application, the limit nearest the anchor, start distance, hit
  testing, edge delta, scroll speed).
- UI integration tests in `src/ui/selection_drag.rs` (real pointer and key
  events on the demo app): a drag previews, shrinks when dragged back and
  applies on release; a click toggles on release and a few pixels of
  movement stay a click; a drag from a selected row deselects; Shift+click
  adds the range; Space, Up, Shift+Up, Shift+Down act on the rows, typing
  reaches the composer, Esc clears; a pointer past the list edge asks for
  the tdesktop scroll step.
- Demo capture `ready-forward` looked at: the tint, the check circles and
  the "Forward 2 / Delete 2" bar render as before.
- Gate (fmt, clippy, core and UI tests), hotspot and file-size checks.

## Not done

- Album rows still have no selection overlay (as before); a drag covers
  their messages by id, so they select as a group.
- A drag that starts on a day separator or the unread divider does nothing
  (tdesktop: the same, those are not items).
- Autoscroll was verified by the step the handler asks for, not by
  watching the list move; the wheel path is the one middle-click
  autoscroll already uses.
