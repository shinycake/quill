# History view polish (date pill, jump fade, service pills)

Reference: Telegram Desktop `history_view_list_widget.cpp` (scroll date),
lib_ui `basic.style` + `history_view_highlight_manager.cpp` (jump fade),
`history_view_service_message.cpp` (service pills). Read-only.

- **Floating date pill.** The kit `MessageScroller` exposes no scroll
  offset, so each history row paints a bounds probe and a viewport canvas
  (painted after the rows) picks the first row below the top edge. The
  scroller entity is observed; each notification (scroll) while not
  tail-following shows the pill, which hides 1 s after the last scroll
  (3 s right after the day changed) with a 200 ms fade. Hidden when the
  top row's inline separator is already at the top edge. Shares
  `pill_label` with the inline separators. Driven by `Instant` plus the
  frame clock only while visible (no `with_animation`).
- **Jump highlight.** `ChatSearchState::jump_serial` is bumped by every
  `begin_chat_search_jump` (search, reply, pinned all use it), restarting
  a stored `Instant`; tint fades in 500 ms, out over 3 s (peak primary
  alpha 0.2). Applied to the row, not the bubble (bubble bounds are not
  available at row build time). Fade is computed at row render time;
  frames are requested at 30 fps only while it runs.
- **Service pills.** `service_pill` (history.rs) wraps Service, TTL,
  screenshot and community rows in a translucent rounded pill. Call rows
  and group-call invitation rows keep their own layouts. Clickable names
  are not done (service text carries no user ids).
- Pure Rust/GPUI only; no platform-specific code.
