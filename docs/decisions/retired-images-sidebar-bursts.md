# Retired images and sidebar redraw bursts

Fixes F1 from the 2026-10 performance sweep and pins down the cause of F3.
F3 itself is not fixed here (see the F3 section).

## F1: FFmpeg video grew memory by about 30 MB/s

### What the audit said, and what was actually wrong

The audit said retired images are only swept from `QuillApp::render`, and
that video frames redraw only the slice or its animation layer, so the sweep
never runs. The second part is wrong. `QuillApp` is an uncached child of
`QuillShell`, which is an uncached child of the kit `Root`, and GPUI renders
uncached views on every window draw. So `QuillApp::render`, and with it
`image_budget::begin_frame` and `sweep`, runs on every frame. The new
`(window frame)` counter in `QUILL_TRACE_TICKS` confirms this: 53–64 window
frames/s while the inline video plays, two animation-layer renders per frame.

The real problem was in how the sweep decided what could still be on
screen. A retired image was kept while any cached slice shown last frame had
last rendered at or before the frame the image was retired in
(`oldest_replayable`). A slice replays the frame it last rendered, so an
image retired after that render might be in the replay. But while a video
plays, the chat list slice (and the conversation slice, when the video is
drawn by the layer) can replay one render for minutes. That pinned
`oldest_replayable` to the old render, so every video frame retired since
then stayed in RAM and in the sprite atlas. Memory only came back when
something re-rendered every slice (a `QuillApp` notify). That is why the
audit saw it bounded by chat switches and by the update stream.

### The change (`src/ui/image_budget.rs`)

GPUI numbers every `RenderImage` from one global counter, so a higher id
means a later image. At the start of each window frame, `begin_frame` records,
for every slice that rendered in the frame just drawn, the next image id
(`images_below`). Every image that render painted already existed at that
point, so its id is lower. A slice replaying that render cannot show any
image with a higher id.

`sweep` now keeps a retired image only when:
- something else still holds it (unchanged), or
- it was retired in the last frame (unchanged: `QuillApp` re-renders every frame), or
- some slice shown last frame last rendered no later than the retirement
  **and** the image existed before that render's frame ended (new; when the
  bound is not known yet, the image is kept as before).

So the check drops only images that the frame on screen cannot reference.
Images that a replaying slice may have painted stay until that slice renders
again, as before. The id comes from making an empty `RenderImage` with no
frames (one atomic increment, nothing allocated), because GPUI does not expose
the counter.

The brief suggested sweeping from the animation-layer tick or from an
`on_next_frame` hook armed by `retire_all`. Neither would have helped: the
sweep already ran every frame, and the problem was what it was allowed to
drop. Nothing under `src/ui/video_player/` changed.

### Measurements

macOS M1 Pro, 120 Hz, release build (`cargo build --release --features ui`),
`ready-video-inline` / `ready-video-note` demos,
`QUILL_VIDEO_BACKEND=ffmpeg` with the sweep's Homebrew-FFmpeg
`libquillvideo.dylib`, `QUILL_ASSUME_ACTIVE=1`, `watch-mem.sh`, sampled
every 5 s. The machine was not quiet, because other agents were building. Load
averages are noted for each run. Memory numbers depend much less on load
than CPU numbers do.

| Run | Footprint | Atlas (IOAccelerator) | load1 |
|---|---|---|---|
| Before, inline, 60 s | 195 → 1.0 GB at 50 s (sawtooth: drops to 451/595 MB at 25/55 s when every slice re-renders), 663 MB at 60 s | 63 → 494 MB | 12.7–13.3 |
| After, inline, 60 s | 109 MB at 5 s, 111 MB at 60 s (flat) | 16.3 MB (flat) | 36–38 |
| Before, round video (sweep-perf run), 20 s | 147 → 386 MB | 39 → 166 MB | 3–86 (sweep) |
| After, round video, 30 s | 97 → 102 MB | 10–12 MB | 26–36 |

Target was < 150 MB over 60 s. The inline run stays at 111 MB. The video kept
drawing at 53–64 frames/s throughout (`QUILL_TRACE_TICKS`).

### Tests

- `frames_retired_while_a_slice_replays_leave_on_the_next_frame`: a slice
  renders once and then replays for 30 layer-only frames while a picture
  is retired each frame. Every picture except last frame's is droppable.
  An image the slice's render may have painted stays until the slice renders
  again. Under the old rule (`at >= oldest_replayable()`), all 30 were kept.
- `a_slice_render_not_yet_closed_by_a_frame_keeps_everything`: during the
  frame of a render the bound is unknown and nothing is ruled out. The next
  frame sets it.

Linux and Windows: the change is platform-independent (GPUI's image ids and
atlas behave the same on every backend). Those platforms use the FFmpeg path
by default, so this is where the leak hit. Not measured on Linux or Windows
hardware.

## F3: update stream redraws the chat list at 120 Hz in bursts (diagnosed, not fixed)

### Cause

I found it with a temporary counter on `Window::request_animation_frame`
callers (`#[track_caller]`, not committed) and the new `(window frame)`
counter:

- `QUILL_DEMO_UPDATE_STREAM=10`: every animation-frame request in the bursts
  comes from `gpui-base/src/motion.rs:651` (`spring`), 37/s. Window frames
  follow a 4 s pattern of 47, 43, 10, 9 per second. Sidebar slice renders are
  40, 40, 3, 3.
- The springs belong to the kit `TabBar` sliding indicator
  (`gpui-component 0.7.3 src/tab/tab_bar.rs`, `render_indicator`, two
  `spring(..., spring_move)` calls, 280 ms). `update_anim_params` retargets
  the indicator whenever the selected tab's measured left or width changes,
  not only when the selection changes.
- In the stream, `updateUnreadChatCount` cycles the main list's unread count
  through 0, 3, 1, 4, 2 (every 40 updates). The folder tab badge appears
  (0 → 3) and disappears (2 → 0) once per cycle. Each time, the "All chats" tab
  changes width, the indicator springs for about 300 ms, and
  `request_animation_frame` re-renders its view, which is the whole chat-list
  slice, at display rate. This matches the audit's "two ~310 ms runs of
  8.3 ms frames" per cycle.
- `ready-typing` alone is clean: 12 window frames/s, animation layer only,
  only the 12 fps typing tick. The chat-list preview emoji lead (GPUI `img`
  animation frames, `chatlist_style.rs`) does not fire in either fixture.

Telegram Desktop animates the active bar only when the selected section
changes (`DiscreteSlider::setSelectedSection` starts `_a_left`/`_a_width`
only when `_selected != index`). An unread badge changing the section width
(`ChatsFiltersTabs::setUnreadCount` → `updateSectionsContentWidths`) moves
the bar without animation.

### Why it is not fixed here

The fix belongs in the kit's `TabBar`: retarget with
`Spring::with_travel(false)` when only the bounds changed and the selection
did not. `gpui-component` is a crates.io dependency that Quill does not
vendor, and `src/ui/folder_tabs.rs` (the caller) is outside this brief's
files. A Quill-side workaround such as re-keying the tab bar would also
reset its scroll and the tabs' own transitions. It needs a decision on
vendoring or patching `gpui-component`.

Baseline numbers for that follow-up (release, `measure-idle.sh`, load1
12–17): active-stream10 5.22% CPU, inactive-stream10 0.51%, active-typing
1.82%, inactive-typing 0.00%.
