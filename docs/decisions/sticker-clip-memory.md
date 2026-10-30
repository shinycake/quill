# Sticker and emoji clips left in memory

Status: done (Linux tests); the Mac numbers are for the lead to measure.
Follows `sweep-r3-browse-memory.md` ("Left to do": about 20 MB left in
libmalloc small regions after browsing, emoji clip frames of about 12 KiB).

## How clips hold memory

Animated stickers and custom emoji play through the streaming player
(`sticker-player.md`). For each open clip (one file at one pixel size) it
holds:

- the compressed frames drawn so far (the store), one LZ4 block per frame:
  20.5 KB on average for a 224 px sticker (measured in
  `sticker-player.md`, 10.5% of the raw frame), and by the same ratios
  (10 to 18%) a few KB for a 64 px panel emoji. All clips share a 96 MB
  budget. Estimated, not measured: a 3 s emoji at 60 frames a second keeps
  about 0.5 MB, so a panel screen of animated custom emoji keeps tens of
  MB.
- the ThorVG canvas and parsed animation, on its worker.
- the frames drawn and not yet shown (up to 8) and the frame on screen,
  each a raw BGRA image (12.25 KiB at 56 px, 16 KiB at 64 px), and their
  atlas tiles once painted.

A clip closes 5 s after the last view drew it, and everything above goes.

## What was wrong

1. **Clips closed only while some view drew.** `Player::trim`, which closes
   clips nobody drew for 5 s, ran only from `Player::frame`, that is from
   a view painting. With nothing animating any more, nothing closed: the
   emoji panel closed over a chat of text, or a scroll through a panel of
   animated custom emoji that ended in a still chat, kept every clip of
   its last 5 s open, with its store, canvas and frames, until some
   sticker drew again or another chat opened (`stop_sticker_playback`).
   Up to the 96 MB budget of stores plus the canvases.
2. **Each stored frame was its own allocation** (`Arc<[u8]>`, 2 to 50 KB).
   On macOS libmalloc keeps freed blocks of that size charged to the
   process (the probe in `sweep-r3-browse-memory.md`: 100% kept for every
   size from 6000 bytes to 1 MiB, below the 64 KiB line of
   `large_alloc`), so a clip's store stayed in the footprint after the clip
   closed.

The "12 KiB emoji clip frames" in the sweep notes are the raw frames of
56 px clips. Those are not kept per clip: each is drawn shortly before it
shows and freed once it has left the screen, about five per clip on
screen, and each must be its own `Vec` (GPUI's `RenderImage` owns its
pixels), so they cannot share a buffer. They hold small regions at their
high-water mark, not more. The per-clip memory that grows with browsing is
the store, which the proposed "one buffer per clip" fits.

## What changed

- `src/ui/sticker_player`: `Player::close_stale` closes the clips no view
  drew for 5 s whose frame is not on screen, and returns how long until
  the next such clip may close. The paint path's trim calls it as before,
  and so does a task (`QuillApp::schedule_sticker_close`) armed at each
  frame of the main window while clips are open, which sleeps until the
  next clip may close. It ends once every open clip is on screen; the next
  frame arms it again. So an idle window has no timer running.
- On screen: each clip records the paint stamp of its last draw, and
  `image_budget::shows_now` tells whether the latest frame drew it or
  replays a slice render that did. A view at rest (a lone emoji that
  played once) inside a slice that replays is not drawn again, but its
  frame is up; closing its clip would show its still at the next render.
  Before, the paint path closed such clips after 5 s while something else
  animated. (`may_still_show` also counts the frame before the latest,
  which suits the start of a frame; between frames it would count a closed
  panel's clips as on screen until another frame.)
- `src/sticker_playback/player/store.rs`: a clip's frames are packed one
  after another into blocks of 256 KiB (a larger frame gets a block of its
  own size). A block is one large allocation, which `large_alloc` gives
  back to the system when it is freed. Stores of closed and evicted clips
  are freed by a worker, outside the engine's lock, so the UI thread never
  waits for the unmapping. A worker unpacks a frame from a shared handle
  to its block; a frame stored while that handle is out goes to a new
  block, which never happens in practice (a clip has one job at a time,
  and an unpacked frame is not stored again).

Cost: 256 KiB of address space per block, of which only the pages written
are charged (the last block of a clip is partly empty); one `mmap` per
block on a worker instead of one `malloc` per frame; copying each packed
frame into its block under the engine's lock (about 1 µs for 20 KB).

## Not changed

- The raw frames of clips under 128 px (above). Rounding them up to 64 KiB
  would move them into the arena at the cost of an `mmap` on a worker and
  an unmapping on the UI thread for every frame, 60 to 120 times a second
  per emoji on screen.
- Chat windows: a clip a chat window draws is stamped with the main
  window's frame count, so it counts as on screen until the main window
  draws again, then closes 5 s after its last draw, as the paint path
  already did.

## Tests

- `sticker_player::tests::a_clip_no_view_draws_closes_with_nothing_drawing`:
  a clip drawn in one frame and not in the next closes 5 s after its draw
  with no view drawing, its store is gone and its frame leaves the atlas.
  It fails without `close_stale` (the clip stays open).
- `sticker_player::tests::a_clip_whose_frame_is_on_screen_stays_open`.
- `image_budget::tests::shows_now_is_the_latest_frame_or_the_slice_it_replays`.
- `sticker_playback::player::tests::a_clips_frames_share_one_large_block`:
  60 frames of a 64 px clip sit in one block of at least 64 KiB and unpack
  to their pixels. It fails with one block per frame.
- `sticker_playback::player::tests::frames_go_to_a_new_block_when_one_is_full_or_being_unpacked`.

## Measuring on a Mac

Release build with `--features demo-capture`, `QUILL_TRACE_STICKERS=1`.
In a chat where nothing animates, open the emoji panel and scroll through
every pack of animated custom emoji (or the sticker panel, hovering many
stickers), close the panel, and idle 30 s. Compare base and this branch:

- The trace: base prints its last line while the panel is open and nothing
  after; this branch prints `clips 0 store 0.0 MB` about 5 s after the
  panel closed.
- `footprint -p` and `vmmap --summary` (MALLOC_SMALL resident): base keeps
  the store that the last trace line shows, plus the canvases; this branch
  should end near its value before the panel opened.
- Then switch to another chat (which closes every clip on both builds) and
  idle 30 s: the difference left is the store held in small blocks, which
  this branch gives back.
