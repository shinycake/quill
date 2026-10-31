# Entity leaks: what a closed surface keeps

## Question

Does closing a box, menu, panel, viewer or window let go of every GPUI
entity, subscription, observer and task that opening it made? A leaked
view keeps its pictures, text layouts and players alive.

## How it is measured

gpui-pre's own `LeakDetector` (`leak_detector_snapshot`,
`assert_no_new_leaks`) only builds for gpui-pre's own tests, with its
`leak-detection` feature, or with `GPUI_LEAK_DETECTION` set at build time.
Quill's builds use none of them, and turning it on rebuilds gpui-pre with
bookkeeping on every entity handle. It also fails on any entity made after
the snapshot, so a cache filled on first open reads as a leak, and it does
not see subscriptions or tasks. Four small hooks were added behind
gpui-pre's `test-support` feature instead
(`third_party/gpui-pre/QUILL-CHANGES.md`):

- `App::live_entity_counts`: live entities by type name. The type is kept
  in its own map, so an entity leased for an update still counts.
- `App::live_subscription_counts` and `Window::live_subscription_counts`:
  observers, event listeners, release listeners, global observers, focus,
  bounds and activation listeners, and so on, by kind. A subscription
  dropped while its emitter was notifying is not counted.
- `ForegroundExecutor::live_tasks`: running foreground tasks by the line
  and column that spawned them (`#[track_caller]` through `cx.spawn`).

`src/ui/entity_leaks_tests.rs` opens a demo (`ScreenshotDemo`), opens and
closes each surface once, takes the counts, opens and closes it three
more times and compares. What stays after the first close is a cache
filled on first use; anything that grows after that is a leak.

After every open and every close the harness draws a frame of each open
window (call, stream, chat and Picture-in-Picture windows too), runs the
test clock a second (`advance_clock`, never wall time) and draws again.
So views a surface makes after its first frame (loaded history, tiles)
are made and closing animations finish. Frames redraw only what changed,
as the screen does. Before counting, with the surface closed, the clock
runs a further minute: a task or timer still alive then never ends
rather than "not done yet". (A minute after every open made the call
test take 21 minutes: `advance_clock` steps through every tick of a live
call.)

Each open and close is checked against what shows (kit dialogs, Escape
layers, panels, menus, Settings and its page, other windows), so a row
that silently fails to open or close fails. The Settings test pins the
System page's `tg:` handler answer (`link_handler::demo_pin`), so no
`xdg-mime` process starts.

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

- Settings: the search input and the volume sliders.
- Chat window: that chat's composer state.
- Media viewer: one `ViewerVideoLayer`.

The user profile kept its groups-in-common search field after closing.
Opening a profile always makes a fresh, empty one, so `close_info_panel`
now drops it (`closing_the_profile_lets_go_of_its_search_field`).

`a_surface_that_keeps_what_it_made_is_named` checks the check itself: a
poll box that keeps a view in a thread-local, detaches a global observer
and a window activation observer and detaches a task that never ends
fails with all four named (`KeptView +3`, `global_observers +3`,
`main window activation_observers +3`, the spawn line). Rows that never
open, or open and never close, fail by name.

## Decision

No leak to fix. The hooks and the table stay as a regression guard: a
surface that starts keeping a strong `Entity` in a closure, a detached
subscription on a long-lived emitter, or a task that never ends fails its
row and names the type, subscription kind or spawn line that grew.

## Out of scope

- Surfaces the demo cannot open (live-only boxes such as payments, or
  anything needing a server reply).
- Memory held outside entities (image and text caches) is covered by the
  other memory slices.
