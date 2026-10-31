# Freed heap goes back once Quill is idle, after any large free (2026-10-31)

## What was wrong

Measured on the owner's Mac (M1 Pro, offline demo, release build): after
scrolling a chat of 1,500 long posts and leaving the app idle, the heap in
use fell to about 52 MB, but the macOS malloc zone stayed at 84.2 MB
resident and 73.8 MB dirty with 49.0 MB allocated. About 25 MB of dirty
memory that was free inside malloc stayed charged to Quill, unchanged
after 10 s and after 40 s of idle. "Malloc Small" dirty stayed at 63 MB.
The footprint fell only about 60 MB for a heap drop of about 80 MB.

Quill hands free heap pages back with `free_heap::release()`
(`malloc_zone_pressure_relief(NULL, 0)` on macOS, `malloc_trim(0)` on
glibc). On main at `04644b4a3` it ran at one place only:

- `BoundedImageCache::idle_trim` called it (`src/ui/image_budget.rs:832`).
- The idle trim runs from the task that `schedule_idle_image_trim`
  spawns (`src/ui/app_slice.rs:220`), and only while `idle_trim_due()`
  (`image_budget.rs:258`) has something due. Something is due 2 s after
  the last image activity: a picture shown that was not shown just before
  (`note_activity`, `image_budget.rs:240`, called at `:888`), or fewer
  cached pictures on screen than at the last frame (`note_departures`,
  `:250`).

The answers to the two questions:

1. **When did the release run?** Only after the pictures on screen
   changed. It did not depend on whether anything was evicted: the trim
   called it every time it ran. A scroll that frees laid-out text
   (`paragraph_cache.rs` lets layouts go at frame boundaries: `settle`
   `:137`, `trim_layouts` `:284`, both from `end_frame` `:376`) but shows
   no new picture got no release. Neither did closed chats' histories
   (`history_prune.rs`, settled from `sync_history_holds` in every frame,
   `app_render/frame_upkeep.rs:20`). Neither did the open chat's window
   trim (`history_trim.rs`). Text and state frees were returned only when
   a picture happened to change afterwards.
2. **Once per idle period, off the hot path?** It was off the per-frame
   path: the release ran in the idle task, on the main thread. It was not
   once: the main window's cache and every chat window's cache each called
   it (`app_slice.rs:241`, `:248`), so N chat windows made N + 1 releases.
   It was not tied to idle either. The task waited 2 s after the last
   picture change, not 2 s after the last frame. It looked at neither
   frames nor the frame clock, so it could run while a scroll went on
   through text or an animation played.

## What triggers the release now

`src/free_heap.rs` (moved from `src/ui/` to the library so that the
session state can feed it) keeps one mark per thread: the bytes freed
since the last release. Everything that frees a lot at once adds to it
with `note_freed`:

- The paragraph cache, for each layout it lets go: a widest line that a
  frame never drew (`Entry::settle`), layouts past the byte or count
  budget (`trim_layouts`), and a paragraph evicted past the text count
  (`Pool::entry`). Each layout's own counted bytes are used.
- History messages: `HistoryMessages::remove` and dropping a whole
  `HistoryMessages`. All of history pruning, the window cap, deletes and
  dropped chats go through these. Each message counts as its
  `size_of::<HistoryMessage>()` (1,416 bytes), a lower bound that leaves
  out its text and media.
- Pictures: entries the image cache evicts (`evict`, used by the scrolling
  trim and the idle trim), retired images the sweep drops
  (`image_budget::sweep`), and the frames of closed sticker clips
  (`sticker_player::app::drop_frames`). The bytes are the frames' pixels.

Not noted: pictures Quill holds outside the caches, such as the viewer's
full-size photo. On macOS those are arena blocks (`large_alloc`) that go
back when freed. On Linux they go back with the next release that other
frees trigger.

`IdleRelease` (`src/free_heap.rs`) decides when. A batch is due once:

- at least `WORTH_RELEASING` (1 MiB) was freed;
- `AFTER_FREE` (2 s) has passed since the batch reached that, so frees
  that come together go back in one release;
- `AFTER_INPUT` (2 s) has passed since the last input: a key, a pointer
  event, a scroll, a window moved or resized. Nothing is released while
  the user scrolls, drags, types or resizes.

Animations do not put it off. The first version of this change also
waited for the frame clock to stop. A looping custom emoji or sticker
keeps a window drawing for as long as it is on screen
(`idle-drawables.md`), so such a window would never have released at all,
which is worse than before. Where the due batch runs depends on whether
anything draws:

- No window began a frame for `QUIET` (400 ms): the task's timer releases
  at once.
- Frames keep coming with no input (animation ticks, a video, a redraw
  that TDLib triggered): the next frame that begins once the batch is due
  defers the release until that frame is drawn and presented
  (`cx.defer` from the frame's render runs after the draw). The release
  lands in the gap before the next tick, at least about 16 ms at 60 Hz.
  The timer never releases while frames come within `QUIET`, because it
  can't tell where a frame falls.

`src/ui/heap_release.rs` wires it in:

- `heap_frame_begins` runs where the main window and chat windows begin a
  frame (`schedule_idle_trims`).
- `note_heap_input` takes input from `note_window_input` (every key
  through `intercept_keystrokes`, and pointer presses, moves and scrolls
  through each window's `scope_probe`), and also takes window moves and
  resizes (the main window's bounds observer, a chat window's `moved`).
- `schedule_heap_release` starts the task once 1 MiB is waiting, from a
  frame or after a free made outside one (after the image idle trim, after
  the sticker closer). The task sleeps until the next look, counts the
  frames that every window began (`Window::frame_number`), and ends once
  the batch went back.

A second window's frame that asked for the same batch finds it gone, so
one release covers everything freed before it.

`QUILL_TRACE_TICKS=1` prints `heap release: N MB freed` each time the
release runs.

## What macOS documents

- `<malloc/malloc.h>`: a NULL zone means that "all zones are examined for
  pressure relief opportunities". A goal of 0 means that "the malloc
  subsystem will attempt to achieve maximal pressure relief". The return
  value is documented as "the number of bytes released". Source:
  [malloc.h](https://github.com/apple-oss-distributions/libmalloc/blob/main/include/malloc/malloc.h).
- `malloc_zone_pressure_relief` in
  [malloc.c](https://github.com/apple-oss-distributions/libmalloc/blob/main/src/malloc.c):
  with a NULL zone it takes the malloc lock and calls `pressure_relief`
  on every registered zone, with the goal of 0 passed through. That covers
  the default zone and the helper zone behind it.
- Nano (blocks of 256 bytes and less), in
  [nanov2_malloc.c](https://github.com/apple-oss-distributions/libmalloc/blob/main/src/nanov2_malloc.c):
  `nanov2_pressure_relief` returns 0 at once unless the nano madvise
  policy is "warning" or "critical". The default policy is "immediate",
  which madvises an empty nano block when it is freed. So the call has
  nothing to do there.
- The scalable zone (tiny, small and medium), in
  [magazine_malloc.c](https://github.com/apple-oss-distributions/libmalloc/blob/main/src/magazine_malloc.c):
  `szone_pressure_relief` runs `tiny_`, `small_` and
  `medium_madvise_pressure_relief`. `CONFIG_MADVISE_PRESSURE_RELIEF` is 1
  on macOS, per
  [platform.h](https://github.com/apple-oss-distributions/libmalloc/blob/main/src/platform.h).
  It also empties the large-block cache. None of this adds to the
  returned total: the large-cache branch adds to a local `total` that
  shadows the one returned. The function therefore returns 0 even when it
  released memory. The "returned 0 bytes" result in
  `sweep-r3-browse-memory.md` does not show that nothing was released.
- `small_madvise_pressure_relief`, in
  [magazine_small.c](https://github.com/apple-oss-distributions/libmalloc/blob/main/src/magazine_small.c),
  moves each region to the depot and madvises the free blocks in it,
  regions that still hold live blocks included. It does this only where a
  free block covers at least one whole page past its free-list header: on
  Apple silicon, 16 KiB. **Free space that is split into pieces between
  live blocks smaller than a page cannot be returned this way.** A normal
  free does not madvise a small block on macOS: aggressive madvise is off
  by default outside iOS.
- libmalloc can also run xzone malloc as the default zone
  (`CONFIG_XZONE_MALLOC` is 1 on 64-bit). Whether a given process gets it
  is decided at launch (`malloc_config_from_input`). These sources do not
  settle whether Quill gets it on macOS 27, or what its pressure relief
  does. `MallocReportConfig=1` prints "Enable Xzone Malloc: 0/1" at
  launch. The sweep-r3 probe found that 1–4 KB blocks went back and
  6,000 B and up did not. That boundary does not match the classic
  zone's tiny/small split (1,008 B), so that probe may not have run on
  the classic zone.

## If relief cannot return the Small pages

If the Mac shows the release running (the trace line) and "Malloc Small"
dirty does not move, the free space sits between live blocks, or the
allocator ignores the call. The next step, not built here, would make a
freed layout free whole pages. Each cached layout's glyphs and clusters
(two thirds of its bytes, `paragraph-cache-widths.md`) would go into one
allocation per layout, sized from `BidiParagraph::heap_size`. A long
post's layout is about 55 KB. Rounded up to `large_alloc`'s 64 KiB, it is
served by the arena, which unmaps its pages when it is freed, as it
already does for pictures. That costs up to one 16 KiB page of rounding
per layout, for at most 2,048 layouts, and it needs a gpui-base change
(`QUILL-CHANGES.md`). Short messages' layouts would stay in malloc.

## Tests

All run on Linux, deterministically, with no wall-clock waits:

- `free_heap::tests` (library) test the policy on fabricated instants:
  one release once quiet, none below 1 MiB, none while input comes and
  one `AFTER_INPUT` after it, and with 60 Hz frames and no input exactly
  one release, asked for by one frame and made once it is drawn, never
  from the timer.
- `state::tests::history_prune::closing_long_chats_asks_for_one_heap_release_once_idle`:
  closing a 1,000-message chat notes exactly 940 messages' bytes, then
  idle releases exactly once.
- `paragraph_cache::widths_tests::what_the_cache_lets_go_is_noted_for_the_heap_release`:
  layouts past the budget and a widest line never drawn are noted, byte
  for byte.
- `heap_release::tests` (demo app, test clock):
  - `a_large_free_goes_back_once_quiet`: none within 1 s, one after, still
    one 30 s later.
  - `a_scroll_in_progress_puts_it_off`: wheel steps every 50 ms for 5 s
    give none, then one within 3 s.
  - `pointer_input_puts_it_off`: pointer moves every 100 ms for 5 s with
    nothing drawn give none, then one.
  - `an_animation_alone_gets_one_release_after_a_frame`: the frame clock
    ticks for 5 s with no input. Exactly one release comes while it plays,
    in a step that drew a frame, and none after.
  - `an_idle_trim_that_freed_nothing_asks_for_no_release`.

Against main (the release back in `idle_trim`, no scheduler, no notes),
the cache, history and first-version UI tests failed: 0 releases where 1
was expected, 1 where 0 was expected, 0 bytes noted. Against the first
version of this change (no release while the frame clock runs, no input
rule), all three input and animation tests fail:

- `pointer_input_puts_it_off` releases while the pointer moves, because
  nothing was drawn.
- `an_animation_alone_gets_one_release_after_a_frame` gets no release.
- `a_scroll_in_progress_puts_it_off` makes none while it scrolls, as it
  should under both. It fails at "once it stopped": the first version
  waited for the frames that the scroll set off to end, about 1 s later,
  and for 2 s after the last free those frames made. It released at
  about 4 s, past the 3 s that the test allows. The new rule releases
  2 s after the last wheel step.

## What only the Mac can confirm

Use the same 1,500-post scroll on main and on this branch, release builds
with `--features demo-capture`, `QUILL_TRACE_TICKS=1` and
`MallocReportConfig=1`. `QUILL_DEMO_STRESS_REDRAW=0` is optional now:
the stress demo's 60 Hz redraw only moves the release to right after one
of its frames. After the scroll, leave the window alone. Run
`footprint -p <pid>` and `vmmap --summary <pid>` at 10 s and at 40 s of
idle. Read the footprint, the "MALLOC_SMALL" dirty row, and the malloc
zone table (allocated against dirty) for both builds.

- Expected if relief works on these pages: one `heap release` line about
  2 s after the last scroll input. The malloc zone's dirty memory falls
  toward its allocated 49 MB, by up to the 25 MB free inside it, and the
  footprint falls by the same.
- If the line shows and the numbers stay where they were, relief cannot
  return those pages: see the section above.
- Check for hitches: a frame trace across the release, with a looping
  emoji on screen. The release should sit between two ticks and delay
  neither.
