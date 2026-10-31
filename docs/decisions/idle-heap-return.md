# Returning freed heap at idle: not worth it (2026-10-31)

**Rejected.** A release of free heap pages once Quill goes idle, after any
large free (laid-out text, history, pictures), saves nothing on macOS 27
and under 10 MB at a time on Linux. The branch stays unmerged:
`claude/idle-heap-return` at `9f7b36ace` has the code, its tests and a
longer write-up.

## The idea

`free_heap::release()` (`malloc_zone_pressure_relief` on macOS,
`malloc_trim` on glibc) runs only from the image cache's idle trim, so
memory freed by a long scroll through text, or by pruning a closed chat's
history, went back only once the pictures on screen changed. The branch
counted the bytes every large free let go and released once a batch of
1 MiB was 2 s old and no input had come for 2 s: at once if no window had
drawn for 400 ms, otherwise right after the next drawn frame.

## macOS 27: the call does nothing

Measured on the owner's Mac (macOS 27.0.1, M1 Pro, release demo build,
1,500-post scroll, two runs each on the branch's first version,
`ad5487c6e`, and on main):

- The release ran as designed, once, 3.6 to 4.6 s after the scroll, and
  took 0.01 ms. It returned 0 and the footprint did not move.
- A standalone probe that allocated 200 MB and freed all of it gave the
  same result, so it isn't fragmentation. Freeing alone took the probe's
  footprint from 202 MB to 75 MB before any relief call.
- The allocator looks like xzone malloc, which marks empty pages
  reclaimable as they are freed. Relief finds nothing left to do.
- Main and that version idled at the same footprint, 134 to 135 MB, with
  87 MB allocated in the malloc zone and only about 14 MB free inside it.

So pressure relief gains nothing on macOS 27. (Blocks of 64 KiB and up
already go back when freed, through `large_alloc`; see
`sweep-r3-browse-memory.md`.) Putting cached layouts in `large_alloc`'s
arena too has little to win with about 14 MB free in the zone; not worth
building.

## Linux: a little, and main returns it later anyway

Two ignored tests on the branch (`src/ui/conversation/heap_return_tests.rs`)
drive Quill's own allocations with glibc 2.39 and `MALLOC_ARENA_MAX=1`, and
read the resident size before and after one release (MB):

| After | Right-to-left posts | Latin posts |
| --- | --- | --- |
| Scrolling through 1,500 long posts | 5.7 | 0.6 |
| Leaving the chat, history cut to 60 messages | 9.9 | 9.0 |
| Emptying the paragraph cache (Quill never does this) | 44.3 | - |

The paragraph cache stays at its 64 MiB budget during a scroll: the 163.7
MB of layouts it let go is turnover that new layouts reuse, not free
memory. Latin text doesn't reach the paragraph cache (gpui's per-frame
cache lays it out). Main's image idle trim returns the same memory at the
next picture change, which real chats make often.

That doesn't pay for freed-byte notes in four places, an input hook in
five and a scheduler. Main's release stays as it is: 0.01 ms, harmless,
and on the classic allocator of older macOS it still madvises free
small-region pages.
