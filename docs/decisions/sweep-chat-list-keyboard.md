# Keyboard navigation in the chat list

Quality sweep item 39: the chat rows had no keyboard navigation. The arrow
keys did nothing in the list, Enter didn't open a chat, and you could only
get from the composer back to the list with the mouse.

## What Telegram Desktop does

- `dialogs/dialogs_inner_widget.cpp` (`processKeyDispatch`, around line
  6618): Up and Down call `selectSkip(∓1)`. PageUp and PageDown call
  `selectSkipPage(visible height, ∓1)`. Home and End call `selectSkip` with
  the total row count, which clamps to the first or last row.
- `selectSkip` with nothing selected selects the first row, whatever the
  direction. So End with no selection lands on the first row, not the last.
- The default list's `selectSkipPage` counts the row it starts on, so a
  page down moves `page - 1` rows. With nothing selected, PageDown selects
  the first row and walks from there, and PageUp does nothing. In search
  results, the page keys skip like the arrows, by a whole page.
- `dialogs/dialogs_widget.cpp` `Widget::keyPressEvent` (line 4768): Enter
  calls `submit()`, which opens the selected row, or selects and opens the
  first row when nothing is selected. Escape calls `escape()` (line 3013),
  which leaves an open folder or forum, then goes back to the open chat,
  then to the first chat folder.
- When the search field has focus, the arrows and page keys still move the
  selection in the results (`processKeyDispatch` runs before
  `redirectKeyToSearch`).
- Opening a chat ends in `HistoryWidget::setInnerFocus`, which focuses the
  message field.
- `history/history_widget.cpp` `HistoryWidget::escape` (line 10841): once
  selection, edit, reply, forward, autocomplete and recording are cleared,
  Escape fires `_cancelRequests`, and the main widget closes the chat. The
  list keeps `_selected`, so the chat you came from stays highlighted.

## What changed

New module `src/ui/chat_list_keys.rs`, with tests in
`src/ui/chat_list_keys_tests.rs`.

- Seven actions (up, down, page up, page down, home,
  end, enter) bound in a `QuillChatList` key context on the sidebar. The
  search field adds a `QuillChatSearch > Input` context with the arrows and
  page keys. That context is deeper than the kit's own `Input` bindings, so
  it wins in the search field and nowhere else. The composer and other
  inputs keep their arrow keys. When the contacts or calls tab is showing,
  or `shortcut_blocked` is true, the handlers call `cx.propagate()`.
- Movement is one pure function, `step`, with tdesktop's rules,
  including the two quirks above (End with nothing selected, and PageDown
  moving `page - 1`). Each quirk has a unit test. A page is the viewport
  height divided by the row height, rounded down, as `selectSkipPage`
  computes it.
- In the list, `VirtualListScrollHandle::scroll_to_item(ix,
  ScrollStrategy::Nearest)` moves only as far as it needs to and is applied
  at the next layout. Search results aren't virtualized: each section
  records its rows' bounds through `on_children_prepainted`. The first row's
  bounds include the section heading, like tdesktop's
  `scrollToFilteredSelected`. The offset is clamped to the scroll range.
- The highlight uses the same fill as the pointer hover (`pressable`'s
  `hover_fill`; the open chat gets its own hover tint), so the keys and the
  mouse look the same, and there is no accent bar. A mouse move over the list hands
  control back to the pointer: the key selection clears, and a hover listener
  that ignores input modality records the row under the pointer. Enter then
  opens that row, as tdesktop's single `_selected` does.
- The list and the search results get the kit's
  `focus_ring_style` only when the sidebar has focus and the last input was
  the keyboard, so a click doesn't draw it.
- Enter opens the selected row, or the first row if none is selected,
  and keeps the selection. On the archive row it opens the archive. In
  select mode it toggles the chat. If the chat has a composer
  (`composer_available`), Enter focuses it; otherwise focus stays in the
  list. Enter in the search field opens the selected result before falling
  back to the old first-result behavior.
- Escape: `cancel_search` still closes the topmost layer first. Reply,
  edit, forward, selection, recording and the composer dialogs are all
  layers, which matches tdesktop's order. With no layer open:
  - From the chat, Escape closes the chat and focuses the list, with the
    chat you came from still highlighted, so Down continues from it.
  - From the list, Escape leaves the archive or community view, else goes
    back to the open chat's composer, else to the first folder tab.

Smaller edits: `session_chat_row` returns `impl ... +
StatefulInteractiveElement` so the list can add the highlight and hover
listener. `search_result_row` returns `Stateful<Div>`. `sidebar` and
`search_results` take the window for the focus ring. The keybinding count
test goes from 94 to 105.

## Left out

- Mouse clicks still don't focus the composer, as before this change.
  tdesktop focuses it on any open. This PR covers
  keyboard opens only, so pointer behavior stays the same.
- Escape in a forum topic list isn't handled specially; it uses the
  same path as the chat list.
- The list keys can't be rebound. They are fixed, like tdesktop's.
  The rebindable chat shortcuts that already existed (next/previous chat on
  Alt+Up/Down, Ctrl+Tab and Cmd/Ctrl+PageUp/PageDown, last chat, and
  Cmd/Ctrl+1..8 for the pinned chats) are unchanged. The existing "Focus
  chat list" shortcut (Cmd/Ctrl+Alt+1) is how you reach the list from the
  keyboard, and it now shows the focus ring.
- Up from the first search result stays on that row. Focus doesn't
  move back to the field, because the field already has it.

## Performance

The extra per-row cost is one `on_hover` listener and one `Option`
comparison in render. Scrolling never touches the key state.

Measurement: `cargo build --release --features frame-profiler` at
`origin/main` (fda1e1e1) and on this branch. The 3000-chat stress fixture
(`QUILL_DEMO_STRESS=3000,0`), 1200x900 window, `QUILL_DEMO_AUTOSCROLL`
scrolling the list for ~1100 frames, `QUILL_FRAME_PROFILE` histograms. Four
runs of each on macOS, interleaved for the last pair.

| run | before draw p99 | after draw p99 | before dirty→present p99 | after dirty→present p99 |
| --- | --- | --- | --- | --- |
| 1 | 6.52 ms | 5.35 ms | 14.97 ms | 11.20 ms |
| 2 | 6.41 ms | 3.68 ms | 13.75 ms | 11.93 ms |
| 3 | 6.91 ms | 7.35 ms | 11.50 ms | 14.04 ms |
| 4 | 9.44 ms | 6.76 ms | 11.91 ms | 14.69 ms |
| median | 6.72 ms | 6.06 ms | 12.83 ms | 12.99 ms |

Draw p50 was 4.39 / 4.63 / 4.60 / 4.67 ms before and 3.38 / 3.19 / 4.73 /
4.58 ms after. The run-to-run spread (3.7 to 9.4 ms p99) is bigger than any
difference between the builds, so I see no regression. I'm not claiming
an improvement either.

## Verification

- 19 tests pass: 6 unit tests for `step` and `rows_per_page`, and 13 GPUI UI
  tests that drive real keystrokes through the dispatcher in
  `chat_list_keys_tests.rs`. The UI tests cover each key in the list (Up,
  Down, PageUp, PageDown, Home, End, Enter, Escape), virtual-list scrolling
  on End and Home (40 extra stress chats), Enter focusing the composer,
  Escape from the composer back to the list with the highlight kept, Escape
  from the list back to the chat, arrows in the composer leaving the list
  alone, a mouse move taking the selection back, the focus ring appearing
  only after keyboard input, and Up, Down and Enter in the search field.
- Light and dark captures of the highlighted row with the focus ring
  (demo-capture build, `QUILL_DEMO_CLICK` to focus the list, then `k:down`).
  I looked at them; they aren't committed.
- Not run: Linux and Windows. The code has no platform branches, and the
  key names (`up`, `pagedown`, `home`, `end`, `enter`) are GPUI's
  cross-platform names.
