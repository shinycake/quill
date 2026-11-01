CHANGES

# Review: history-render (29d7f31) against 8fd661682 (redraw-slices)

Commits reviewed: 7638692 (stop animating what the media viewer hides), cb29a61 (build hover buttons only under the pointer), 29d7f31 (rest the date pill on a timer).

The behaviour holds up. I found no user-visible bug in the three changes. One finding blocks merge because the PR's CI fails.

## Findings

### 1. The hotspot CI check fails on `src/ui/mod.rs`

- File: `src/ui/mod.rs`, line 484 (commit cb29a61)
- Change: `mod scroll_restore_tests;` became `pub(in crate::ui) mod scroll_restore_tests;`, so the new date-pill and row-hover tests can import `open`, `select`, `frames` and `A`.
- Failure scenario: open the PR against redraw-slices. The `check-hotspots` step (`.github/workflows/ci.yml:38`) runs `bash scripts/check-hotspots.sh <base>` and fails. I reproduced this locally:
  ```
  HOTSPOT src/ui/mod.rs: a normal PR may only: add or remove a `mod` line for a new UI module
      +pub(in crate::ui) mod scroll_restore_tests;
  ```
  The PR can't merge green. None of the three commits has a `Hotspot-change:` trailer.
- Fix: leave `src/ui/mod.rs` alone. One way is to move the shared demo helpers (`open`, `select`, `frames`, `A`) into a test-support module that `history_fx_tests.rs` and `row_hover_tests.rs` import. Another is to re-export them through a module that is not a hotspot. If the visibility change really is needed, add a `Hotspot-change: <reason>` trailer to cb29a61's replacement commit.

## Checked and found sound (no finding)

Hover buttons:
- **Layout.** Mounting or unmounting the corner buttons does not move the row. The buttons sit in the kit's `BubbleReactions` slot, which is `absolute()`, so they take no flow space. `actions_span` only offsets that absolute slot. `row_hover_tests::only_the_row_under_the_pointer_has_its_buttons` asserts that the row centre does not move.
- **Scroll under a still pointer.** `rows_painted` hit-tests in paint, after this frame's hitboxes are inserted (`mouse_hit_test` is recomputed after prepaint). It then defers a notify, so the buttons follow on the next frame. The test covers this.
- **Keyboard and touch.** `Hitbox::is_hovered` returns false after keyboard input. The new code excludes touch, which matches the old `group_hover` path (gpui `div.rs` ~3573 excluded touch too). This is not a regression.
- **Pointer leaving the window.** There is no `MouseExitEvent` handling. The old `group_hover` path had none either, so this is unchanged.
- **Screen reader and UI agent.** `a11y_tree_wanted` includes `set_a11y_forced`, so every row keeps invisible buttons in the tree (`TreeOnly`). `set_a11y_forced` calls `window.refresh()`, which re-renders the cached slices.
- **Message menu.** It opens at the click position from app-level state (`open_message_menu`), so unmounting the button does not drop it.
- **Callers of `render_history_row`.** There is a single caller, the list that paints the `rows_painted` canvas, so `painted` is drained every frame and never grows.

Viewer cover:
- **Close path.** Every close path goes through `close_media_viewer` (or the passcode lock), which calls `cx.notify()` on `QuillApp`. `observe_self` then notifies every slice (`app_slice.rs:272`), so slices whose ticks were refused render again and ask again. `cover_tests` shows the stickers playing again after close.
- **Read receipts and new messages.** These arrive by notify, not by frame-clock ticks, and `tick_hidden` only filters ticks. Nothing that reaches the conversation by notify stops behind the viewer.
- **Chat windows.** They keep their own `viewer` (swapped in by the slot), and `under_viewer` only matches main-window slices. A viewer in a chat window therefore doesn't freeze the main window, and the reverse holds too.
- **Message menu over the viewer.** It is rendered by `QuillApp` (`app_render/mod.rs:819`), not by a covered slice, so it keeps its ticks.

Date pill:
- **Hide deadline.** `hide_after` is only reset while the pill is hidden, and `last_scroll` only moves later. An armed `wake` therefore always fires at or before the fade-out is due, and the "only looked again" re-arm covers later scrolls (`a_later_scroll_moves_the_fade`).
- **Stale day after a resting scroll.** I wrote a scratch test (not committed): rest the pill on day A, then scroll so day B's row is at the top, with no forced frames. The pill showed day B, so the label is not stale.
- **Chat windows and `spawn_main`.** `wake_date_pill` uses `spawn_main`, which would clear the main window's `wake` if it ran in a chat-window scope. Chat windows never call `ScrollDate::scrolled`, because `note_history_scroll` is main-only, so the pill never shows there and this can't happen today. If the pill is ever enabled in chat windows, use `spawn_scoped` instead.

## What ran

- `cargo test --features demo-capture --bins -- row_hover history_fx media_viewer::cover layer_tick agent_driver_open chat_window::tests::a_reply_started wheel_ui scroll_restore`: 38 passed, 0 failed.
- `cargo test --features demo-capture --test structure_checks`: 10 passed.
- `cargo fmt --check`: clean.
- `scripts/check-app-name.sh`, `check-file-size.sh`, `check-lang.py`, `check-window-scope.py`: OK.
- `scripts/check-hotspots.sh 8fd661682`: **fails** (finding 1).

## Not run

- Clippy (both feature sets) and the full test suite.
- Running the real app or taking screenshots. Hover, the viewer fade and the date pill are only covered by the demo UI tests above.
- Real touch, screen-reader (VoiceOver, NVDA or Orca) and multi-monitor checks are not possible in this headless container.
