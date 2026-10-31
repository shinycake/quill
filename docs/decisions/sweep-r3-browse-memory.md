# Idle memory after browsing (sweep round 3)

Status: done. The numbers below come from the arena allocator in this PR
(build `fix3`, built from the arena allocator commit; the allocator
has not changed since).

The question: after a session of browsing (photos, the media viewer, the
emoji and sticker panels, a few chats), does Quill go back to roughly its
launch footprint once it sits idle, and if not, what holds the memory?

## Method

Measured on an M1 Pro (16 GB), macOS 27.0.1, release builds with
`--features demo-capture`, the offline `ready-chats` screenshot demo only
(throwaway data root under `$TMPDIR`). Scripts live outside the repo in
`/Volumes/Idan1TB/quill-work/scratch/sweep-r3-browse-memory/`
(`browse-idle.sh`, `browse-path.sh`, `ab-final.sh`, `final-medians.py`,
`final-vm.py`; raw runs in `runs/final*`).

- Window 1200x740. Chat A is the animated history (inline video, round
  note, custom emoji, TGS and WebM stickers, spoilers) with 120 photos of
  2560x1920 (`QUILL_DEMO_STRESS_PHOTOS`) at its bottom; chat B is text only.
- The run opens chat B, idles 60 s (launch idle), browses with
  `QUILL_DEMO_CLICK`, then leaves chat B open and idles 65 s (post idle).
- `footprint -p` (phys_footprint, what Activity Monitor shows) every 5 s;
  `vmmap --summary` near the end of each idle period.
- Browse variants:
  - full: scroll up through about 24 photos, open the viewer and step
    through 12 photos, scroll back, open the emoji panel and its Stickers
    and GIFs tabs, then visit three chats.
  - viewer: three scroll steps, then the viewer through 12 photos.
  - photos: scroll up 24 steps and back down.
  - panel: the emoji panel, its sticker and GIF tabs.
  - chats: switch between four chats.
- Base (origin/main at `f094957c2`) and fixed builds ran one after another,
  alternating per variant, never side by side (overlapping demo windows
  occlude each other and stop drawing), on a display of their own. Three
  rounds; the third was cut short by the Mac going offline after the
  photos variant, so panel and chats have two runs each. Runs made later,
  after the second display was gone, had the demo window behind other
  windows and never drawing; they are kept as `runs/occluded-*` and not
  used.
- Caveat: other agents' builds ran on the same machine throughout. Under
  that memory pressure the kernel sometimes reclaims the memory libmalloc
  holds, which is why the base runs spread so widely (full: 141 to 295 MB).
  The fixed runs spread much less (full: 113 to 136 MB).

## Results (MB, medians)

| Variant | Build | Launch idle | Browse peak | Post idle | Post runs |
|---|---|---|---|---|---|
| full | base | 84 | 413 | 293 | 293, 295, 141 |
| full | fixed | 71 | 290 | 114 | 136, 113, 114 |
| viewer | base | 88 | 373 | 220 | 257, 220, 89 |
| viewer | fixed | 70 | 287 | 95 | 95, 96, 93 |
| photos | base | 88 | 425 | 255 | 143, 255, 331 |
| photos | fixed | 69 | 254 | 92 | 92, 91, 99 |
| panel | base | 88 | 114 | 94 | 106, 83 |
| panel | fixed | 69 | 109 | 88 | 89, 87 |
| chats | base | 86 | 144 | 80 | 92, 69 |
| chats | fixed | 74 | 140 | 68 | 67, 69 |

Where the post-idle memory sits (vmmap dirty + swapped, medians, MB;
"arena" is the allocator's live blocks, vmmap's "App-Specific Tag 1"):

| Variant | Build | Freed large blocks | Malloc small/tiny | GPU atlas | Arena |
|---|---|---|---|---|---|
| full | base | 107 | 59 | 72 | 0 |
| full | fixed | 0 | 29 | 42 | 12 |
| viewer | base | 102 | 56 | 26 | 0 |
| viewer | fixed | 0 | 24 | 30 | 11 |
| photos | base | 130 | 68 | 22 | 0 |
| photos | fixed | 0 | 24 | 26 | 11 |
| panel | base | 0 | 42 | 22 | 0 |
| panel | fixed | 0 | 28 | 26 | 4 |
| chats | base | 0 | 41 | 10 | 0 |
| chats | fixed | 0 | 22 | 13 | 4 |

An earlier base run of the full path with no other builds running held
374 MB after 65 s idle, 178 MB of it "Malloc Large (empty)".

### Cost

The arena keeps no freed blocks, so every large block is fresh pages and
pays a page fault per page on first touch, where libmalloc hands back
pages it already has. One allocate, fill, free cycle (`allocbench/`,
which compiles `src/large_alloc.rs` unchanged; median of 5 alternating
rounds, `runs/allocbench.txt`; other builds were running, load average
about 12):

| Block | Cycle | Arena (µs) | libmalloc (µs) |
|---|---|---|---|
| 64 KiB | fill | 9.1 | 1.2 |
| 64 KiB | touch pages | 7.8 | 0.1 |
| 196 KiB sticker frame | fill | 18.4 | 3.4 |
| 196 KiB sticker frame | touch pages | 15.0 | 0.2 |
| 1 MiB | fill | 71.3 | 17.0 |
| 1 MiB | touch pages | 54.4 | 0.6 |
| 14.7 MB decode buffer | fill | 1012.3 | 239.9 |
| 14.7 MB decode buffer | touch pages | 743.3 | 5.6 |
| 19.7 MB photo | fill | 1348.5 | 319.9 |
| 19.7 MB photo | touch pages | 999.9 | 7.6 |

Browsing puts none of these on a per-frame path: sticker and emoji clips
decode all their frames once into the playback cache and replay them,
inline video and round notes play CoreVideo buffers that Rust never
allocates, and photos decode on background threads, where a few
milliseconds sit next to tens of milliseconds of JPEG decoding. One path
does allocate per frame: screen sharing in a call converts each captured
frame with `nv12_to_i420` (`src/calls/capture/screen.rs`, called from
`screen_mac.rs` for every frame), about 3.5 MB per 1080p frame, which now
pays the fault cost above each time. Reusing that buffer is a follow-up.

CPU with the animated history open was measured for the first design
(which kept up to six freed blocks for reuse): base 1.97, 2.63, 2.70 s;
first design 2.00, 2.68, 2.79 s over 30 s, within the run-to-run spread.
The same A/B for the arena could not be run: by then the second display
was gone and the demo window never drew (both builds read 0.00 s, kept
as `runs/occluded-final-cpu.txt`). `cpu-final.sh` repeats it.


## Causes

1. **libmalloc keeps freed blocks charged to Quill.** On macOS 27, a freed
   block of a few KB and up stays in the process footprint until the
   kernel happens to reclaim it. A C probe (allocate 64 MB in blocks of
   one size, free them all, read phys_footprint) kept 100% for every size
   from 6000 bytes to 1 MiB, and `malloc_zone_pressure_relief(NULL, 0)`
   returned 0 bytes for each; only 1–4 KB blocks went back. Decoded photos
   (`src/ui/image_decode/rust_codecs.rs`, full-size RGB intermediates of
   about 14.7 MB), the viewer's full-size image, sticker frames (224 px,
   196 KiB each) and evicted cache entries all land there. This was the
   biggest share: up to 178 MB after the full path.
2. **The image cache never went idle after leaving a chat of photos.**
   `BoundedImageCache` trims to its idle budget (8 MB) 2 s after the last
   "activity", but only a newly shown image counted as activity
   (`src/ui/image_budget.rs`, `load_entry`). Switching to a text chat shows
   no new image, so the cache stayed at its 48 MB scrolling budget (plus
   their atlas tiles) for as long as Quill sat there. An instrumented run
   confirmed the last idle trim ran while chat A was still open.
3. **Glyphs pinned image textures in the GPU atlas.** GPUI frees an atlas
   texture only when its last tile goes, and places a new tile in the
   newest texture with room, whatever its kind. Colour emoji glyphs (never
   removed) landed in the space photos left in their textures, so those
   textures (4 MB and up each) stayed alive for good. An atlas dump after
   the full path showed six polychrome textures holding only 18 emoji each
   at 8% use. Same code in the Metal, wgpu and DirectX backends.

## What changed

- `src/large_alloc.rs` (new, macOS only, registered in `src/lib.rs`): the
  global allocator serves blocks of 64 KiB to 1 GiB from an arena of
  address space reserved at the first large allocation, and gives their
  pages back the moment they are freed. Details and the review that shaped
  it are under "The allocator" below. Smaller blocks, larger ones and
  alignments above 64 KiB stay with the system allocator.
- `src/ui/image_budget.rs`:
  - `note_departures` and `BoundedImageCache::trim`: fewer cached images
    that may be on screen than at the previous frame counts as activity, so
    the idle trim runs 2 s after a chat of photos is left.
  - `idle_trim` calls `free_heap::release` every time, not only when it
    evicted something (a closed viewer's photo or a chat's sticker frames
    were freed meanwhile). On Linux that is `malloc_trim`; on macOS the
    zone pressure relief, for what is left in small blocks.
- GPUI atlas (`third_party/gpui-pre/src/platform.rs`,
  `gpui-pre-apple/src/metal_atlas.rs`, `gpui-pre-wgpu/src/wgpu_atlas.rs`,
  `gpui-pre-windows/src/directx_atlas.rs`): `AtlasBackend::insert_image`,
  called for `AtlasKey::Image`; each backend texture records whether it
  holds images and `allocate` only uses textures of the same kind. Each
  crate's `QUILL-CHANGES.md` describes it.

## The allocator

A global allocator is the riskiest code in the app: every allocation in
every thread goes through it, and a mistake corrupts memory far from the
cause. The first version (the one whose CPU was measured, see "Cost")
mapped each large block with `mmap`, unmapped it on free, and kept
up to six freed blocks in a reuse cache behind a spin lock. Reviewing it
before the PR turned up two problems serious enough to redesign it:

1. **It trusted the caller's size to decide whose block it was.**
   `dealloc` sent a block to `munmap` or to `free` by `layout.size()`, and
   unmapped `size` rounded up. Rust's contract says that size is exact, but
   not every crate keeps it: coreaudio-rs (used by cpal for every
   microphone and system-audio capture on macOS) frees its input buffers
   with `Vec::from_raw_parts(ptr, len, len)` where the real capacity can be
   larger. With the system allocator that is harmless (`free` ignores the
   size). With the first version, a buffer of 64 KiB or more freed with a
   smaller size went to `free` (which does not own it), and one freed with
   a different large size unmapped the wrong length or entered the cache
   with the wrong length, which the next same-size request would then
   overrun. An 8-channel interface at 2048 frames has 64 KiB buffers.
2. **The spin lock could hang the process.** A thread that forks (or takes
   a signal whose handler allocates) while another thread holds the lock
   leaves the child (or the handler) spinning forever. It also spun at
   whatever QoS the waiter had, so a background thread holding it could
   stall the UI thread.

The rest held up: alignments above the page went to the system allocator,
zero-size requests never reached the mapped path, `mmap` failure returned
null, and `realloc` copied the smaller size each way across 64 KiB. TDLib
and the other C and C++ libraries call `malloc` themselves and never saw
the mapped blocks.

The design now (`src/large_alloc.rs`):

- **An arena of address space.** At the first large allocation Quill
  reserves 120 GiB of inaccessible address space (one `mmap` with
  `PROT_NONE`; it costs no memory, but the process's virtual size, VSZ in
  `ps` and Activity Monitor, grows by those 120 GiB, which bug reports
  should not read as a leak). It is split into 15 size classes,
  64 KiB to 1 GiB in powers of two, each with 8 GiB cut into slots of its
  size (131072 slots of 64 KiB, eight of 1 GiB).
- **A block is a slot.** Allocating claims a free slot and replaces it with
  a fresh readable, writable mapping (`mmap` with `MAP_FIXED` inside the
  reservation). Freeing replaces it with an inaccessible mapping again,
  which drops its pages at once, then lets the slot be claimed again. The
  arena is never unmapped, so no other mapping can land inside it. Pages
  never touched cost nothing: a 70 KiB block in a 128 KiB slot is charged
  80 KiB, as before.
- **Ownership by address.** `dealloc` and `realloc` decide whether a block
  is the arena's by a range check on the pointer and take the slot, class
  and length from the address; the caller's `Layout` is never used for
  that. A wrong size frees the right slot, and a `malloc` block handed to
  Rust's `dealloc` still goes to `free`. An address inside the arena that
  is not a slot's start is a caller bug either way: `dealloc` leaks it,
  and `realloc` returns null, which Rust's collections treat as
  allocation failure and abort on.
- **No locks.** Slots are claimed with `fetch_or` and released with
  `fetch_and` on static bitmaps (32 KiB). Nothing allocates, nothing waits,
  so `fork` or a signal at any point cannot leave anything held. A slot's
  bit is cleared only after its mapping was replaced, so a slot is never
  handed out while its old pages are still in use.
- **Fallback, never failure.** If the reservation fails, a class runs out
  of slots, or a mapping fails, the block comes from the system allocator
  instead. Nothing in the allocator can panic (no indexing, no unwrap; the
  arithmetic is bounded by the arena's constants).
- **`realloc`** stays in place within a size class (growing needs nothing,
  the slot is mapped whole; shrinking drops the pages past the new end),
  and otherwise allocates, copies the smaller size (never more than the
  slot holds) and frees. A system block that grows to 64 KiB moves into the
  arena; an arena block that shrinks below it moves to the system
  allocator.
- **No reuse cache.** Every block is fresh zero pages, so `alloc_zeroed`
  costs nothing extra and a freed block's memory is gone immediately, not
  at the next idle. Its cost is under "Cost" above.
- `System` is used with exactly the layout the caller gave for every block
  outside the arena, which is its contract; a block moves between the two
  only through `realloc`'s copy.

Checked against the dependency tree for C code that frees Rust memory or
Rust code that frees C memory: Quill's own code has none (every
`Box::into_raw`/`CString::into_raw` comes back through Rust); objc2's
`NSData::from_vec` and core-foundation's `CFData::from_arc` free through
Rust callbacks; `NSData` and `CFData` built from slices copy. A C library
calling `free` on a Rust block of 64 KiB or more would still be wrong; none
was found.

Rejected alternatives for the allocator itself:

- `malloc_zone_pressure_relief`: returns 0 bytes on macOS 27 (see Rejected).
- `madvise` on chosen buffers only (decode intermediates, the viewer
  image): the decoded images live in `image::RgbaImage` (`Vec<u8>`) inside
  GPUI's `RenderImage` and are freed wherever the last `Arc` drops, so
  every image path in Quill and GPUI would need a custom buffer type, and
  sticker frames and file reads would still be missed.
- Keeping the first version with a `malloc_size` ownership check: it fixes
  a `malloc` block freed with a large size, but not an arena block freed
  with a small or different size, which is the coreaudio-rs case.

## Tests

- `src/large_alloc_tests.rs` (the arena is the test binary's allocator
  too, so `Vec` goes through it):
  - `a_freed_full_size_photo_is_unmapped` fails with the system allocator
    (the freed 19.7 MB stays resident) and passes now.
  - Growing a `malloc` block into the arena and shrinking an arena block
    out of it keep the contents, and the old slot's pages are gone.
  - `realloc` within a size class stays in place and drops the pages past
    a shrunk end.
  - Alignments up to 64 KiB come from the arena and are met; 256 KiB goes
    to `System` and is met there, through `realloc` too.
  - Zero-size requests never reach the arena; `alloc_zeroed` is zero in a
    reused slot.
  - `dealloc_trusts_the_address_not_the_layout`: a `malloc` block freed
    with a large layout goes to `free`; an arena block freed with a 100
    byte layout (the coreaudio-rs case) still drops all its pages. The
    first version sent that block to `free`.
  - Freeing an interior pointer leaks it and keeps the block; a full size
    class falls back to `System`; an impossible reservation and a 64 TiB
    request return `None` and null without panicking.
  - Eight threads allocating, filling, growing and checking blocks at
    once never see each other's bytes.
- `image_budget::tests::leaving_a_chat_of_photos_schedules_the_idle_trim`.
- `metal_atlas::tests::test_glyphs_never_keep_an_image_texture_alive`
  failed with the kind filter disabled and passes now. The wgpu and DirectX
  copies of the test could not run here (vendored crates are not built
  for those targets on macOS, and CI does not run vendored crate tests).

## Rejected

- `malloc_zone_pressure_relief` (already called at idle): returns 0 bytes
  on macOS 27 for every size tested, and the footprint doesn't move either
  (`idle-heap-return.md`).
- `MallocLargeCache=0` / `MallocSpaceEfficient=1`: undocumented
  environment switches read at process start; not something an app can
  set for itself.
- A separate `malloc_create_zone` for images: the zone caches the same way.
- A threshold below 64 KiB: 16 KiB pages make small blocks cost up to 2.6x
  their size, and the 12 KiB emoji frames would need a page each.

## tdesktop

tdesktop calls `malloc_trim(0)` at most every 10 s when the event loop is
about to block on Linux (`platform/linux/integration_linux.cpp`, glibc
only), clears the sticker panel's decoded data when it hides
(`StickersListWidget::clearHeavyData`), and unloads history media more than
two screens away (`kUnloadHeavyPartsPages = 2` in
`history/history_inner_widget.cpp`). It has nothing like the macOS
allocator change; Qt images sit in the same libmalloc.

## Verification

Photos evicted by the idle trim load again: chat A scrolled up through the
photos and back, chat B open for 8 s (the idle trim runs 2 s after the
photos leave and evicts down to the 8 MB idle budget, logged with
`QUILL_TRACE_TICKS`), then back to chat A and up through the photos again.
Captured in light and dark with a demo-capture build of this branch
(`verify-reload-deep.sh`). The trim log shows 23 cached images (36 MB)
dropping to 3 (8 MB) while chat B was open, and the cache back up to 17
after the way back. Every photo on screen in both captures is drawn; none
is left as a placeholder.

## Left to do

- Remaining growth: about 20 MB in libmalloc small regions (emoji clip
  frames of 12 KiB and similar blocks, freed but kept), and the GPU atlas
  ending 4–16 MB above launch (glyph textures). Emoji clips could keep
  their frames in one allocation per clip so they cross the 64 KiB line.
- The animated-history CPU A/B for the arena (`cpu-final.sh`), once a
  display is free for the demo window.
- Reuse the screen-share conversion buffer (`nv12_to_i420`) and measure
  CPU in a call.
- Rate-limit `free_heap::release` on idle trim to at most once per 10 s, as
  tdesktop does with `malloc_trim`.
- A macOS CI job that runs `cargo test --lib large_alloc` and clippy.
- `src/ui/image_budget.rs` has 12 lines of headroom under the file-size
  limit; split it before its next change.
