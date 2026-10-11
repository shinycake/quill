# Statistics: real charts, zoom, message and story pages

Parity item: `parity:admin-stats-messages`.

## What Telegram Desktop does

`info/statistics/` and `statistics/`:

- The chat page has an overview (2×2 values with a green or red change
  against the previous period), then every graph TDLib returns, each drawn
  by a kind tdesktop picks per graph slot (Linear, Bar = step line,
  StackBar, DoubleLinear = two scales, StackLinear = 100% areas).
- A chart has a header (title, visible dates), the plot with rulers and
  date labels, a range selector under it (mini chart, two handles, drag
  inside to move, click outside to jump) and line switches when there is
  more than one line. The last shown line cannot be switched off.
- Pressing the plot shows a details box (date, each line's value, shares
  on percentage charts). On a graph with a zoom token the box has a
  chevron; clicking it calls `getStatisticalGraph(token, x)` and the day's
  chart replaces the overview with a "Zoom Out" button.
- Async graphs (`statisticalGraphAsync`) are requested while the page
  fills and appear once loaded; error graphs are left out.
- Channels list recent posts (10, then 30 more per "Show more") with
  views, shares and reactions; a row opens that post's or story's
  statistics. Groups list top members, admins and inviters.
- Message and story statistics: a preview, Views / Public shares /
  Reactions / Private shares, the Interactions (DoubleLinear) and
  Reactions (Bar) charts, and the public forwards list.
- The message context menu offers "Statistics" on channel posts.

## What Quill had

The panel from #706 showed overview rows and a unicode sparkline for four
graphs. Story statistics (viewer panel) used the same sparklines. No message
statistics, no async loading, no zoom, no recent-post previews.

## What changed

Core (`quill`, tested on Linux CI):

- `src/stat_chart/`: Telegram's chart JSON parser (columns, names,
  `KEY#hex` colors, hidden lines, `subchart.defaultZoom`/`show`,
  `percentage`, week tooltips) and the math ported from tdesktop: start/end
  index search, y limits per kind (double-line ratios, stack sums, 0..100),
  nearest point and bar under the pointer, the range selector (handles in
  pixels, min gap, centre-on-click, resize keeps the range), rulers
  (`ChartRulersData` min-height mode, right captions for the second scale),
  power-of-two date label steps, the legend's last-line rule, and labels
  (`12.3K`, `12,345`, `+645 (5.5%)`, `Sat, 5 Oct 2024`, percentages that sum
  to 100).
- Requests `getMessageStatistics`, `getMessagePublicForwards`,
  `getStatisticalGraph`; parsing `messageStatistics` and bare
  `statisticalGraph*`; `messageProperties.can_get_statistics`; public
  forwards keep their view count.
- `StatsState` (in the groups domain): graph fetches keyed by
  (chat, token, x) and matched by request id, zoomed graphs, the open post's
  statistics and forwards, recent-post previews (`getMessage`), the panel
  page, and the recent-posts count. Opening the panel resets it to the
  overview.
- `ConnectDriver`: `load_stat_graph`, `zoom_stat_graph`,
  `fetch_message_statistics` (plus the first forwards page),
  `load_more_message_forwards`, `retry_message_statistics`,
  `fetch_stats_post`.

UI:

- `ui/stat_chart/`: a `RenderOnce` chart with its interaction state in
  `Window::use_keyed_state` (no `QuillApp` field). Drawn on a GPUI canvas:
  stroked paths for lines and steps, quads for stacked bars, filled polygons
  for areas, captions shaped with the text system. Built series are cached
  per (rect, range, y limits, line alphas, selection) so hover redraws replay
  them; the mini chart has its own cache. Y-scale and line-opacity changes
  ease over 240 ms from the sampled value; the chart's own 16 ms clock
  notifies only its state (so only the panel redraws) and stops when the
  change ends. Power saving's "Animations" switch makes changes instant.
- gpui-kit's `component::chart` was checked: its charts are single-scale
  with no range selection, no two-scale lines and no 100% stacking, so they
  could not draw these kinds; the chart is custom, using theme tokens for
  chrome and the data's colors (tdesktop's palette keys) for lines.
- Interaction: hover moves the details (the brief asked for hover; tdesktop
  shows them on press), a press pins them, pressing the pinned point lets go,
  leaving the plot clears an unpinned point, moving the range clears it.
  The tooltip sits beside the point on the roomier side. Keyboard: the plot
  takes focus; Left/Right step through points, Enter zooms, Escape clears.
  Legend switches are focusable checkboxes.
- `statistics.rs` (panel, overview, chart slots with tdesktop's titles and
  kinds, top member lists), `statistics_graph.rs` (graph → chart, async
  loads deferred out of render, zoom wiring, overview grid),
  `statistics_posts.rs` (recent posts, message and story pages, public
  shares, "Go to message"). The message menu gains "Statistics" after Pin.
- The story viewer's statistics panel and the poll results draw real charts
  (DoubleLinear + Bar, Linear) instead of sparklines.

## Differences from tdesktop, on purpose

- Day labels and titles read in UTC (the statistics are UTC days); times
  within a day read in local time. tdesktop reads both in local time, which
  shifts every day label back one day west of UTC.
- A ruler set whose short captions would repeat ("12K" six times on a
  zoomed day) is spelled out (`12,000`, `12,018`, …).
- An hourly series within one day titles that day once, not
  `1 Oct 2024 — 1 Oct 2024`.
- Moves of the range selector jump instead of animating the window; the y
  scale still animates.

## Not done

- The local zoom of percentage charts into a pie chart (tdesktop
  `StackLinearChartView::maybeLocalZoom`).
- Recent-post thumbnails (rows are text, views, reactions and shares).
- The poll votes chart inside message statistics (poll stats keep their
  own dialog) and "Export to File".
- Ruler cross-fades and date-label fades when the step changes.
- `group_panels` still asks `getChatStatistics` with `is_dark: false`;
  colors come from the palette keys either way. Left alone because another
  slice owns `src/ui/group_panels/`.

## Verification

- Core: 16 `stat_chart` tests (every chart shape parses, errors, default
  zoom, indices, y limits per kind, nearest point and bar, selector drag and
  clamps, rulers incl. two scales and spelled-out captions, label steps,
  labels and titles incl. time zones and weeks, numbers, percentages, last
  line), 3 request builder tests, 7 driver/reducer tests (async graph once,
  zoom + failure + retry, message statistics page + forwards paging, late
  answers dropped, retry, post previews, recent-post paging, the menu flag),
  forwards view count.
- UI (`statistics_tests.rs`, gpui test window): every chart kind draws at the
  panel width while error graphs stay out; hover shows the tooltip, a legend
  click and a selector drag work and the drag hides the tooltip; a press and
  a tooltip click zoom, the day's chart replaces the overview, zoom out
  restores it; ten recent posts list, a row opens the message page with its
  chart and public shares.
- Demo captures (temporary demo, not committed) checked: overview with
  deltas, line, two-line, stacked bar with selection, 100% areas with
  shares in the tooltip, two-scale lines with coloured captions, step chart,
  zoomed day with "Zoom out", message page with public shares.
- Gate (fmt, clippy core + UI, core + UI tests), `check-hotspots.sh`,
  `check-file-size.sh`.
