# Entity leaks: what a closed surface keeps

## Question

Does closing a box, menu, panel, viewer or window let go of every GPUI
entity, subscription, observer and task that opening it made? A leaked
view keeps its pictures, text layouts and players alive.

## How it is measured

gpui-pre's own `LeakDetector` only exists under `cfg(test)` of gpui-pre
itself, so Quill's tests cannot use it. Four small hooks were added
behind gpui-pre's `test-support` feature (`third_party/gpui-pre/QUILL-CHANGES.md`):

- `App::live_entity_counts`: live entities by type name. The type is kept
  in its own map, so an entity leased for an update still counts.
- `App::live_subscription_counts` and `Window::live_subscription_counts`:
  observers, event listeners, release listeners, global observers, focus,
  bounds and activation listeners, and so on, by kind.
- `ForegroundExecutor::live_tasks`: running foreground tasks by the line
  that spawned them (`#[track_caller]` through `cx.spawn`).

`src/ui/entity_leaks_tests.rs` opens a demo (`ScreenshotDemo`), opens and
closes each surface once, takes the counts, opens and closes it three
more times and compares. What stays after the first close is a cache
filled on first use; anything that grows after that is a leak. Each open
and close is checked against what shows (kit dialogs, Escape layers,
panels, Settings, windows), so a row that silently fails to open cannot
pass. Timers and closing animations run on the test clock
(`advance_clock`), never wall time.

## Result

No surface grows. Checked, all with no growth over three further opens:

- Main window: emoji, sticker and GIF panels, reaction picker, message
  menu, chat menu, send menu, send effects, search, search in chat, poll
  box, new group, new channel, edit profile, forward box, chat theme box,
  clear history box, channel edit box, invite link box, emoji status
  picker (Premium preview on a non-Premium demo), jump to date, new
  folder, shared media, user profile, channel info, profile from an
  avatar, media viewer (photo), separate chat window.
- Every Settings page (18).
- Switching through every demo chat and back.
- Story viewer, call window, live stream window.
- Media viewer playing a clip, and Picture-in-Picture
  (`src/ui/video_player/tests/entity_leaks_tests.rs`).

Kept after the first close and reused (bounded, not leaks):

- User profile: the common-groups search input (two `TextareaState`),
  kept by `close_info_panel` (`src/ui/group_panels/mod.rs`).
- Settings: the search input and the volume sliders.
- Chat window: that chat's composer state.
- Media viewer: one `ViewerVideoLayer`.

`a_surface_that_keeps_what_it_made_is_named` checks the check itself: a
poll box that keeps a view in a thread-local, detaches a global observer
and detaches a task that never ends fails with all three named (`KeptView
+3`, `global_observers +3`, the spawn line).

## Decision

Nothing to fix. The hooks and the table stay as a regression guard: a
surface that starts keeping a strong `Entity` in a closure, a detached
subscription on a long-lived emitter, or a task that never ends fails its
row and names the type, subscription kind or spawn line that grew.

## Out of scope

- Surfaces the demo cannot open (live-only boxes such as payments, or
  anything needing a server reply).
- Memory held outside entities (image and text caches) is covered by the
  other memory slices.
