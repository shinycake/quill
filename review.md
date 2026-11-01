APPROVE

# Review 2: history-render (f6d75d5), changes since 29d7f31

The first review (branch claude/review-history-render) approved the behaviour of 7638692, cb29a61 and 29d7f31. This review reads only what changed after that:

- 82686cf, the merge of main (5a173f5) into `src/ui/frame_clock.rs`
- 0c35f8a, the date pill wake moved from `spawn_main` to `spawn_scoped`
- 69bcb26, the test helper move that fixes the earlier hotspot finding (checked only as far as the hotspot check)

I found no blocking issue.

## Tick loop merge (`src/ui/frame_clock.rs`)

The merge keeps all three parts the commit message names:

- **Main's own-window branch.** `request_tick` gates on `tick_wanted(...) && !own_window_active`. The loop reads `app_ticks` and `own_window`, and a tick with only `own_window` set marks and notifies `window_views` alone. This matches 5a173f5 line for line.
- **This branch's `tick_hidden` filter.** It replaces main's `main_covered && slices.owns(id)` in both places: the early return in `request_tick` and the `views` filter in the loop. `tick_hidden` still contains that test as its first term and adds the viewer cover term, so the merged code drops at least as much as main.
- **Main's chat window hold.** `windows.holds_view` stays as a second early return and a second `.filter`.

**No tick is lost.** Compared with each parent, the only extra filtering is the other parent's own filter. `layers_drawn.ticked` still runs for every target before filtering, as it did in both parents. `tick_app` (`chat_window/cover.rs:282`) is the same in all three revisions. A `None` target never reaches either filter.

**No hidden view draws.** A covered slice or a slice under an opaque viewer is refused when it asks and dropped again at the tick. A held chat window view is refused the same way. One path skips both filters: the trailing `for id in window_views { app.notify(id) }`. It comes from main unchanged. Its only callers are `request_window_view_tick` in `group_call_panel.rs:164` and `group_call_speaker.rs:92`. Those views live in the group call window, which is never a main window slice, never under the viewer and never a chat window. So neither filter could match them. This merge adds no new exposure.

## Date pill wake (0c35f8a)

`spawn_scoped` gives the task a `ScopedApp` carrying `windows.scope` from when `wake_date_pill` ran. `ScopedApp::update` runs the closure inside `in_window_scope(scope, ...)`. `swap_slot` (`chat_window/slot.rs:134`) swaps `history`, so a wake set in a chat window scope sets `history.scroll_date.wake = None` on that chat window's `ScrollDate`, not the main window's. If the chat window closed first, `in_window_scope` returns `None` and nothing runs. The stale `wake` goes away with the slot. For the main window the scope is `None`, which is what `spawn_main` gave, so behaviour there is unchanged. `check-window-scope.py` passes.

### Note (not blocking)

After clearing the wake, the task calls `notify_conversation`. That reads `self.slices.conversation`, and `swap_slot` does not swap it. In a chat window scope the task would clear the right state but notify the main window's conversation slice, so the chat window's pill could stay visible until something else redrew it. This can't happen today. `ScrollDate::scrolled` is only called from `note_history_scroll`, which runs in `in_window_scope(None, ...)`, so a chat window's pill stays `DateMotion::Hidden` and never arms a wake. If the pill is ever enabled in chat windows, the wake should redraw the window in scope as well.

## What ran (at f6d75d5)

- `cargo test --features demo-capture --bins -- frame_clock layer_tick history_fx row_hover media_viewer::cover chat_window::cover scroll_restore`: 54 passed, 0 failed.
- `cargo fmt --check`: clean.
- `scripts/check-hotspots.sh origin/main`: OK. The earlier review's finding 1 is fixed by 69bcb26.
- `scripts/check-window-scope.py`: OK.

## Not run

- Clippy (both feature sets) and the full test suite.
- The real app. The cover, viewer and pill paths are only covered by the demo UI tests above.
