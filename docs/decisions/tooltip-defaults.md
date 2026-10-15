# Tooltip timing follows Telegram Desktop

## What Telegram Desktop does

- Every tooltip waits 1000 ms before it shows: `Ui::Tooltip::Show(1000, ...)`
  in lib_ui `ui/widgets/tooltip.cpp` (`InstallTooltip`) and the same literal in
  `history/history_inner_widget.cpp` (link tooltips), `history/view/history_view_list_widget.cpp`,
  `chat_helpers/emoji_list_widget.cpp`, `chat_helpers/bot_keyboard.cpp`,
  `inline_bots/inline_results_inner.cpp`, `info/media/info_media_list_widget.cpp` and others.
- Leaving the trigger calls `Tooltip::Hide()` at once (`tooltip.cpp`).
- There is no "recently shown" shortcut: moving to a neighbouring trigger waits the full delay again.

## What changed

- `src/ui/tooltip_timing.rs` installs `TooltipDefaults` (GPUI Kit 0.7.3) from
  `QuillApp::new_with_demo`, which both the app and the screenshot demos go through, so
  `src/main.rs` (a hotspot) stays untouched: show delay 1000 ms, grace period 0
  (Quill's kit patch hides at once; a zero grace period also drops the delay skip).
- The hand-drawn link tooltip in `src/ui/entity_links.rs` (not a kit tooltip: it is anchored
  to the pointer) had its own 450 ms constant. It now reads the installed
  `TooltipDefaults` show delay, so link and kit tooltips cannot drift apart.
- Kit tooltips with no per-trigger override now take 1000 ms instead of the kit's 500 ms.

## Verified

Unit test for the installed values and for `show_delay` reading the global (`tooltip_timing` tests).
Not verified visually: the delay is a timer, covered by the kit's own tooltip tests.
