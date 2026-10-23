# History pages load three screens early

Owner report (2026-10-23, Quill 2026.1023.3674): scrolling a channel felt
sticky at times.

## What was measured

Release build with `frame-profiler`, M1 Pro, 120 Hz, the `ready-chats` demo
with `QUILL_DEMO_STRESS=50,1500`, 52 photo messages (1280 px JPEGs) and
`QUILL_DEMO_AUTOSCROLL=800,450,30,200,1400` (1,400 wheel events up and down
the history), `QUILL_DEMO_STRESS_REDRAW=0`:

| histogram | p50 | p90 | p99 | max |
| --- | --- | --- | --- | --- |
| draw | 3.21 ms | 3.73 ms | 4.02 ms | 52.76 ms (first frame) |
| dirty to present | 6.47 ms | 9.74 ms | 13.46 ms | 13.95 ms |
| input latency | 5.49 ms | 9.08 ms | 13.57 ms | 13.99 ms |

Drawing a scrolled frame takes about 4 ms at worst, inside the 8.3 ms a
120 Hz frame allows. Rendering loaded rows is not what makes scrolling stick.
The demo has every message loaded, so it cannot show the other cause.

## The cause

The older page was asked for only when row 0 of the loaded window was built
(`maybe_auto_load_older` from the row renderer), which happens once row 0 is
within the list's 400 px overdraw of the viewport. A scroll up therefore ran
into the top of the window and stopped there for a `getChatHistory` round
trip (often a network fetch in a channel), and again for every page. Scrolling
down in a window that stops short of the latest message (a channel opened at
its unread messages) prefetched only 8 rows ahead.

Telegram Desktop asks while three screens are still left in either direction
(`HistoryWidget::preloadHistoryByScroll`, `kPreloadHeightsCount = 3`).

## Change

- `ListState::estimated_distance_to_edges` (gpui-pre) and
  `MessageScrollerState::distance_to_edges` (gpui-component): the viewport's
  distance to the window's start and end, rows not laid out yet counted at the
  mean row height, and the viewport height.
- `QuillApp::preload_history_by_scroll` runs on every history scroll and asks
  for the older or newer page while that distance is at most three viewport
  heights (`preload_due`). The driver already drops a request while one for
  the same edge is in flight and stops at the start of the chat. The
  row-0 and 8-row triggers stay for windows that change without a scroll.

## Tests

`src/ui/history_preload_tests.rs`: the distance on a real scroller at the
tail (rows above never laid out) and mid-list, and the three-screen rule.

## Not covered

No live channel was scrolled: the demo has no server, and the owner's account
is not used for tests. Separate chat windows keep the row-0 trigger.
