# Overlay ticks and player scans notify and scan less

Round 3 audit items 5 and 9. No tdesktop behavior changes; nothing on screen
differs.

## What changed

**Story viewer.** The overlay is now a cached slice (`SliceKind::Story`,
`story_slot` in `app_slice.rs`). Its 100 ms tick calls `notify_story`, which
notifies that slice only, instead of `cx.notify()` (which notifies every slice
through `observe_self`). Any other `QuillApp` notify still refreshes the story
slice, so the overlay stays a pure function of app state.
The tick's custom-emoji work (`story_custom_emoji_fetch_ids`, the sticker fetch
and `ensure_story_custom_emoji_downloads`) moved into
`warm_story_custom_emoji`, which keeps the last `(story, ids needed, stickers
known)` in `StoryUi::emoji_warm` and runs only when that changes. The record is
cleared when the viewer closes.

**Call clock.** The 1 s tick calls `notify_call_clock`: the open chat's
conversation (the call bar shows the clock), the chat windows, and the call
window's `CallPanel` through its entity id (kept in `CallUi::panel_view`, a
shared cell the window root fills in, so no `QuillApp` borrow is needed while
the window is built).

**Viewed ids.** `render_history_row` and `report_visible_history` use
`notify_tick(SliceKind::Conversation)` (which also targets a chat window's own
view). `report_visible_history` notifies only when `view_messages` queued a
request.

**Player state.** `playlist_audio_ids` is kept per `(chat, messages.revision())`
in `PlaylistCache` (`player_bar.rs`). `revision` moves on any change to the
loaded messages, so the cached list is what a fresh scan returns.
`pump_translation` asks `PumpScan` (`translate_ui/pump_scan.rs`) for its
candidates: the whole window when the chat, target language, automatic flag or
skip list changed, otherwise the messages that wanted a translation last pass
plus `changed_since(last revision)`. Stored translations only ever turn
"wanted" into "done", so this gives the same ids as walking the whole window.

## Behavior notes

- A story's custom-emoji download that failed is not retried on every 100 ms
  tick any more; it is tried again when the story, its needs or the known
  stickers change, or the viewer reopens.
- Nothing here claims a speed-up; the numbers were not measured.

## How verified

- Unit tests: `PlaylistCache` keyed by chat and revision; `PumpScan` against a
  full walk with a monotone filter, including new, edited and removed messages.
- UI tests with the slice render trace: the story tick renders only `story`; an
  app notify renders it too; the call clock renders `call-panel` and
  `conversation` but not `sidebar` or `(app notified)`; the playlist follows
  added and deleted audio messages and is reused in between; the emoji warm-up
  runs once per story.
- `/verify` captures in light and dark of `ready-story-viewer`, `ready-audio`
  (player bar) and `ready-video-call` (call window and call bar), read back by
  eye. The capture shows one frame: it cannot show that the tick keeps
  updating, which the tests cover.
