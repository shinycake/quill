# Statistics follow-up: tdesktop details, pie zoom, thumbnails, poll votes, export

Follows `codex-stats.md` (#722). No new parity item: these complete
`parity:admin-stats-messages` against Telegram Desktop.

## Details on press, like tdesktop

`ChartWidget::setupDetails` shows a point's details on a mouse press and
follows the pointer while the button is held. Pressing the shown point again
hides them, moving the range hides them, and the box fades in and out over
200 ms. Quill now does the same. Hover alone shows nothing. Left and Right
step through points, Enter zooms and Escape hides. The box sits beside the
point, on the side with more room.

## Label behavior: what stays and what was reverted

- **Days in UTC: kept.** tdesktop's own code treats x values as UTC days.
  `LangDetailedDayMonth` decides "day or hour" from
  `dateTime.toUTC().time()`, the export writes them as UTC Excel dates
  (`ms / 86400000 + 25569`), and the hourly chart is titled "Views by hours
  (UTC)". tdesktop then displays them with `base::unixtime::parse` in local
  time, which shows the previous day everywhere west of UTC. That
  contradicts its own intent, so it counts as a bug. Times within a day stay
  in local time, as in tdesktop.
- **Ruler captions: reverted.** Above 10,000 they are shortened as tdesktop
  does, even when that repeats a caption (`12K` on a zoomed day).
- **Hourly titles: reverted.** They name both ends, as tdesktop's
  `HeaderSubTitle` does: `1 Oct 2024 — 1 Oct 2024`.
- **Range selector: now animates.** A click outside the window slides it
  there over 150 ms with sine in-out (`moveCenterAnimation`).

## Ruler and date fades

When the y limits change, a new ruler set fades in over the old one, the way
tdesktop's `ChartRulersView::add` does. While dragging, a new set is added at
most every 320 ms, matching `kHeightLimitsUpdateTimeout`; in between, the
newest set is updated in place. When the date step changes, the old labels
fade out over 200 ms while the new ones fade in, as in `updateBottomDates`.
Both use the chart's own frame clock: a 16 ms timer that notifies only that
chart's state while something animates and stops when nothing does. The
"Animations" power-saving switch makes every change instant.

## Pie zoom of percentage charts

A percentage chart (`StackLinear`) zooms locally, not through the server
(`hasLocalZoom`). Clicking a day's details turns the areas into a pie.

- **Selector window.** The range selector then spans eight days around the
  clicked day, with that day selected (`maybeLocalZoom`, `kLimitLength`).
- **Footer.** The selector shows those days as 100% columns
  (`paintZoomedFooter`).
- **Slices.** The shares cover the selected days. They are rounded to whole
  percents that add up to 100 (`PiePartsPercentage`) and slide to new values
  over 150 ms.
- **Labels.** Each label is scaled by its share and pulled inward
  (`paintPieText`). Slices under 3.9% get no label.
- **Hover.** Hovering a slice moves it out by 8 px and shows the line's total
  over the selected days in a box at the top right (`PaintDetails`).
- **Leaving.** The header shows the days and "Zoom out"; Escape works too.

The core math is `quill::stat_chart::pie`: `local_zoom`,
`local_zoom_indices` (`FindStackXIndicesFromRawXPercentages`), `pie_parts`
and `pie_part_at`. It is unit-tested.

One deliberate simplification: tdesktop morphs the areas into the circle.
Quill fades the areas out while the pie grows in from 60% size and fades in
over 400 ms, ease-out circ, the same duration and curve.

## Thumbnails in recent posts

tdesktop's `MessagePreview` draws a post's photo or video thumbnail rounded,
and a story's preview in a ring, in a slot that stays even for text posts so
the rows line up. Quill draws the same: a 40 px rounded image, or a circle
with an accent ring for stories.

`StatsPostPreview` keeps the post's thumbnail candidates (the same choice as
reply previews), and a fetched post registers its files. While the panel is
open, `Session::stats_thumb_file_ids` adds the thumbnails of the listed posts
and stories, and of the open post or story, to the thumbnail download pass.

## Poll votes in message statistics

For a poll post, tdesktop's `fillPollVotesGraph` adds a "Poll votes" line
chart below the other charts. Quill asks `getPollVoteStatistics` once per
post (`fetch_stats_poll_votes`) and shows the chart when it arrives. TDLib
gates this request on `messageProperties.can_get_poll_vote_statistics`, and
those properties are not loaded for a post opened from statistics. So the
request is sent anyway and a refusal simply leaves the chart out, which has
the same effect as the gate.

## Export to File

tdesktop writes an Excel workbook (`info_statistics_export.cpp`,
`statistics_sheets.cpp`, `statistics_xlsx.cpp`), and Quill writes the same
one.

- **Sheets.** An "Overview" sheet comes first: Value and Previous columns for
  chats, and plain counters for posts and stories (a negative counter is left
  blank). Then there is one sheet per chart that has data.
- **Chart columns.** The first column is a Date, a DateTime for intra-day
  charts, or a Number named "Hour" for hour-of-day series, followed by one
  column per line.
- **Excel rules.** The first row is frozen. Headers are bold and
  right-aligned over numeric columns. Columns get sized widths. Sheet names
  follow Excel's rules (no `:\/?*[]`, at most 31 characters, ` (2)` for
  repeats).
- **Packaging.** The parts are inline-string SpreadsheetML zipped in memory
  with deflate (flate2, already a dependency). The suggested name is
  `<chat>_<post or story>_<date>.xlsx`. After saving, the status line says
  "Statistics saved to <file>." or "Could not save statistics.".
- **Where it is.** "Export to file…" appears on the overview (next to
  Refresh) and on loaded post and story pages; tdesktop puts it in the
  section's menu.

The chart table (`chart_slots`) moved to core, so the panel and the export
share titles and kinds.

## Group panel fixes

The statistics request now passes `is_dark` from the theme. Opening the panel
resets it to the overview in demo mode as well: both the live driver and the
demo path go through `Session::set_open_info_panel`.

## Verification

- **Core:**
  - 23 `stat_chart` tests: pie parts and angles, slice picking, the local
    zoom window and indices, and the workbook's zip entries, CRCs, XML,
    sheet names, the chat and post sheets, file names, and hour and
    date-time columns.
  - 9 driver tests, including poll votes asked once and thumbnail candidates
    only while the panel is open.
- **UI tests (gpui test window):**
  - Hover shows nothing and a press shows the details.
  - Pressing again hides them after the fade, and a range drag hides them.
  - The languages chart zooms into a pie, a slice shows its total, and
    Escape zooms back out.
  - The overview and message page workbooks contain the right sheets,
    including the async graph that loaded.
  - The earlier zoom and recent-post tests still pass.
- **Captures:** the pie zoom with labels and selector columns, and the
  message page with press details, from a temporary demo that is not
  committed.
- **Scripts:** gate, `check-hotspots.sh`, `check-file-size.sh`.

## Not done

- tdesktop's area-to-circle morph; Quill uses a grow-and-fade.
- Post thumbnails in the demo fixtures (there are no media files there).
  The download wiring is covered by the driver test.
