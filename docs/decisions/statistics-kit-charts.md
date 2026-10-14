# Statistics charts on gpui-kit

The owner called the hand-drawn statistics charts ugly and asked for gpui-kit's chart components, as the revenue charts got in `revenue-and-affiliates`. This covers every chart the statistics panel draws: channel, group, message and story statistics (`statistics_graph.rs`, `statistics_posts.rs`, `story_insights_ui.rs` all go through `StatChartView`). The boosts page has no chart, only a progress bar, so it is unchanged. There is no parity item: this is design debt.

## What tdesktop does

`lib_ui`'s `statistics/` widgets draw five kinds from the graph JSON:

- `LinearChartView` for lines, and the same view with a step for "bar" type lines.
- `StackBarChartView` for stacked columns.
- `StackLinearChartView` for stacked percentages, which zooms into a pie (`PieChartView`) over a week around the clicked day.
- `DoubleLinearChartView` for two lines on two value scales.

Each chart has a header with the shown range, six rulers, a footer minimap with a draggable range, and legend chips that switch lines off. Pressing a point shows a details box with every line's value. Pressing the box zooms into that day when the graph has a zoom token.

## What Quill draws now

- Lines and step lines are a kit `AreaChart` with transparent fills, and `linear()` or `step_after()` per series.
- Stacked percentages are an `AreaChart` with the series stacked from the top down in opaque fills. A click on a day opens a kit `PieChart` of that day, with a range slider over tdesktop's window of days around it (`local_zoom_window`, four before and five after) and a "Zoom out" button.
- Stacked columns are `StackedBars`, a plot built on kit primitives. The kit `BarChart` draws one series and fits its own domain, so it cannot stack. `StackedBars` uses what `BarChart` uses: `ScaleBand`, `ScaleLinear`, the `Bar` shape with a base, `Grid`, `PlotAxis`, `PlotLabel` and the kit `Tooltip` with a band cross line. It looks the same as kit bar charts.
- Two-scale charts are two `LineChart`s stacked as small multiples, each with its own value axis and the date axis on the lower one only. The dataviz rules forbid a dual axis on one plot. When a line is switched off, the remaining one gets the full height.
- The range selector is a dimmed `AreaChart` preview of the whole period over a kit range `Slider`. On a two-scale chart, each line fills the preview on its own scale.
- Legend chips are kit `Toggle`s with a color swatch. The name stays in a text token, never the series color.
- Hovering a point shows the kit tooltip: the date, then a row per line with a color stroke, the name, the value and, on percentage charts, the share. A chevron marks a point that zooms. A click zooms into the day (or the pie).
- Keyboard: a click or Tab focuses the plot. The left and right arrows pick a day, shown in a readout box under the plot. Enter zooms into it and Escape drops it or leaves the pie. Escape is handled as the app's cancel action on the focused plot, and it falls through to close the panel when there is nothing to drop.
- Value axes are rounded out to round, whole-number steps (`nice_domain`), keeping tdesktop's six rulers, and labels sit outside the plot so no text lands on a mark. The two-scale plots put their labels in one fixed-width column so both plots' days line up.
- The plotted points are built once per change of lines or range and cached in the chart state, since every hover frame redraws the panel.

The old painter (`stat_chart/{details,elements,paint,pie_paint,state_zoom,tween}.rs`) and the pieces of `quill::stat_chart` only it used (x fractions, ruler captions, pie angles, local zoom indices) are deleted. `range.rs` became `lines.rs` with `toggle_line` only.

## Deviations from tdesktop

- Details show on hover in the kit tooltip instead of on a press. Keyboard users get the same content in the readout box.
- Rulers are rounded to round numbers, so 11,437 reads as 11,400. tdesktop splits the raw min and max into five steps.
- No y-scale animation when the range or lines change. The kit charts redraw without it.
- The two-scale chart is two plots instead of one plot with two axes.
- The footer's handles are the kit slider's, not tdesktop's bracket handles.

## Secondary text (WCAG AA)

Measured from the demo captures, in Quill's default look:

- Axis labels use the kit's `muted_foreground`: `#737373` on `#fafafa` in light (about 4.5:1) and `#a3a3a3` on `#0a0a0a` in dark (about 7.8:1).
- Subtitles and tooltip line names use `text_muted()`: `#59636e` on `#fafafa` in light (about 5.9:1) and `#8b949e` on `#0a0a0a` in dark (about 6.4:1).

Both pass AA. As noted in `revenue-and-affiliates`, a tdesktop palette theme maps `muted_foreground` to its `windowSubTextFg` (Day/Classic `#999999`, about 2.8:1). That token is global, so this change leaves it alone.

## Verification

- `ready-statistics` screenshot demo (new) with `QUILL_DEMO_FIRST_CHART` to scroll to later charts. Before and after captures in light and dark were reviewed locally and are linked from the PR.
- UI tests in `statistics_tests.rs`: every kind draws; arrow keys read out a day and Escape drops it without closing the panel; a legend chip switches a line; hover and click zoom into a day, which loads and unzooms; hover and click open the languages pie and Escape leaves it; export and recent posts are unchanged.
- Unit tests: `nice_domain`, `local_zoom_window`, and the stacked points and value domains in `ui/stat_chart/series.rs`.

Not verified: dragging the range slider in a test (the kit slider is covered by gpui-kit's own tests), and screen-reader output beyond the plot's group role and label.
