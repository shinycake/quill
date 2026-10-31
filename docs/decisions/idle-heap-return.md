# Freed heap goes back once Quill is idle, after any large free (2026-10-31)

**Outcome: not worth merging** ([Recommendation](#recommendation)). The
release runs as designed, but it saves nothing on macOS 27 and under
10 MB at a time on Linux, memory that main also returns, later.

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
  say what its pressure relief does. `MallocReportConfig=1` prints
  "Enable Xzone Malloc: 0/1" at launch. The sweep-r3 probe found that
  1–4 KB blocks went back and 6,000 B and up did not. That boundary does
  not match the classic zone's tiny/small split (1,008 B), so that probe
  may not have run on the classic zone. On macOS 27.0.1, Quill appears to
  get xzone, and its relief does nothing (measured below).

## Measured on the Mac: no saving on macOS 27

The owner's Mac on macOS 27.0.1: the 1,500-post scroll on `ad5487c6e` (the
first version of this change) and on main, two runs each.

- The trigger works as designed. One `heap release: 193.9 MB freed` line
  came 3.6 to 4.6 s after the scroll ended, and the call took 0.01 ms.
- `malloc_zone_pressure_relief` does nothing there. It returned 0, and
  the footprint did not move.
- A standalone probe that allocated 200 MB and freed all of it got the
  same result, so fragmentation is not the cause. Freeing alone took the
  probe's footprint from 202 MB to 75 MB, before any relief call. The
  allocator looks like xzone malloc, which marks empty pages reclaimable
  as they are freed, so relief finds nothing left to do.
- Main and this branch idle at the same footprint, 134 to 135 MB. The
  malloc zone had 87 MB allocated and only about 14 MB free inside it.

So this change saves no memory on macOS 27. The call stays: it costs
0.01 ms once per idle period, and on the classic allocator of older macOS
it still madvises free small-region pages (above). The step that the
first version of this doc proposed for when relief can't return the
small pages (one allocation per cached layout, from `large_alloc`'s
arena) isn't worth building: with about 14 MB free inside the zone, it
has little to win.

## Measured on Linux

`src/ui/conversation/heap_return_tests.rs` (two ignored tests beside the
paragraph cache's scroll tests) runs Quill's own allocations in the test
build:

1. The demo's group gets 1,500 posts of 110 words, a number and a link.
   The test scrolls through them newest to oldest, a frame each, as
   `paragraph-cache-widths.md` measured.
2. It opens another chat and deletes the group's messages down to the 60
   newest. That frees what the prune frees for a closed chat. (The demo
   keeps every history, so the prune itself does not run.)
3. It empties the paragraph cache. Quill never does this: the step shows
   the most that laid-out text could give back.

After each step, the test reads the resident size (`/proc/self/statm`)
and malloc's heap in use (`mallinfo2`), calls `free_heap::release()`, and
reads the resident size again. Setup: glibc 2.39, with `MALLOC_ARENA_MAX=1`
so that the test thread uses the main arena, as Quill's UI thread does.

Right-to-left posts, which the paragraph cache lays out (MB):

| After | Resident | Heap in use | The release gives back |
| --- | --- | --- | --- |
| The posts loaded | 149.2 | 14.2 | |
| The scroll (163.7 MB of layouts let go on the way) | 221.7 | 77.1 | 5.7 |
| Leaving the chat (history cut to 60) | 216.4 | 66.9 | 9.9 |
| Emptying the paragraph cache | 206.5 | 4.3 | 44.3 |

Posts in Latin letters don't reach the paragraph cache: gpui lays them out
in its own per-frame cache. The scroll grew the resident size by 1.1 MB,
and the release gave back 0.6 MB. Leaving the chat freed 8.9 MB of heap,
and the release gave back 9.0 MB.

What the numbers mean:

- After a long scroll, about 6 MB comes back. The paragraph cache stays at
  its 64 MiB budget. The layouts that replace the ones it lets go reuse
  most of their memory, so the 163.7 MB that it noted is turnover, not
  free memory.
- Leaving a chat of 1,500 long posts gives back about 10 MB.
- Only freeing all laid-out text at once would give back much (44 MB),
  and Quill never does that.
- Main releases at none of these steps. Its image idle trim releases
  once the pictures on screen change, which in real chats (avatars,
  photos, stickers) happens often. The same memory then comes back,
  later. What this branch gains on Linux is timing, in stretches with no
  picture changes.

## Recommendation

Don't merge this branch. It saves nothing on macOS 27, and on Linux it
returns under 10 MB at a time, which main returns too at the next
picture change. That does not pay for its bookkeeping: freed-byte notes in
four places (paragraph cache, history, image cache, sticker frames), the
`IdleRelease` policy, input hooks in five places, and a module to schedule
the release.

Keep what was learned instead. A doc-only change to main could carry this
file, mark the left-to-do item in `sweep-r3-browse-memory.md` as done
with this outcome, and add a comment at main's `release()` saying that it
does nothing on macOS 27. Main's release (once per window, after image
activity) costs 0.01 ms on the Mac and is otherwise harmless.

## Tests

All run on Linux, deterministically, with no wall-clock waits. The two
measurements above are ignored tests that print instead of checking.

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
