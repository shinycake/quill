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

A clip closes 5 s after no window shows it any more, and everything above
goes.

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

- `src/ui/sticker_player`: clips close between frames, never while one
  paints. A task (`QuillApp::ensure_sticker_closer`), started when a view
  opens a clip and running while any clip is open, calls
  `Player::close_stale`, which closes the clips no window has shown for
  5 s and says when the next clip off screen is due. The task looks again
  after the next frame of any window or when that clip is due, each plus
  1 s (`CLOSE_SLACK`): a burst of frames, or clips due close together,
  cost one look, and with nothing drawn and nothing due it waits with no
  timer. It ends once no clip is open. The paint path no longer closes
  clips: a frame paints its views one by one, and a view at rest that the
  frame had not painted yet looked gone and closed (it then showed
  nothing, then its still).
- On screen, per window: each draw stamps the clip with where it was
  drawn, one stamp per window and slice. In the main window that is its
  paint stamp (`image_budget`'s frame count, and the slice that may replay
  the draw); in a chat window, that window's own frame count
  (`Player::frame_starts`, called as each window starts a frame). A clip
  is on screen while the latest frame of some window drew it, or replays a
  slice render that did (`image_budget::shows_now`); a closed window shows
  nothing (`Player::window_closed`). A view at rest (a lone emoji that
  played once, a landed dice) is not drawn again while nothing changes,
  but its frame is up: closing its clip would show its still at the next
  draw, then play it again. Stamping chat-window draws with the main
  window's frame count, as the first version did, kept every clip a chat
  window drew open while the main window was minimized or covered (macOS
  then stops drawing it), and closed a chat window's clips at rest once
  the main window drew.
- When the 5 s start: each look notes the clips on screen as seen then,
  and the first frame after a look notes them again (a view at rest may
  have been up for minutes), so a clip closes 5 s after the frame that
  left it out, give or take the slack.
- The frames of the clips it closes: no window shows them (it looks
  between frames), so each one nothing else holds leaves every window's
  atlas and memory there and then (`App::drop_image`). Retired instead,
  they waited for the sweep at the start of a main-window frame, which an
  idle app never draws. One still held elsewhere is retired as before. A
  view showing another size's frame while its own size opens stamps that
  clip too, since that frame is on screen.
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
frame into its block under the engine's lock (about 1 µs for 20 KB). The
closer looks at most about once a second while windows draw (a pass over
the open clips); at each frame start the player only checks whether the
closer waits.

## Not changed

- The raw frames of clips under 128 px (above). Rounding them up to 64 KiB
  would move them into the arena at the cost of an `mmap` on a worker and
  an unmapping on the UI thread for every frame, 60 to 120 times a second
  per emoji on screen.
- A window that stops drawing (minimized, or fully covered) keeps the
  clips of its last frame open until it draws again or closes: at most
  one screen of them. It may be shown again any time, and closing them
  would show their stills then.
- A frame drawn again after its stored copy failed to unpack is stored
  again; the bad copy stays in its block, counted, until the clip closes.
  An unpack fails only on data the store did not write.
- A new block is allocated under the engine's lock: one `mmap` of a few µs
  per 256 KiB of frames.

## Tests

- `animated_emoji::tests::emoji_no_window_shows_close_and_free_their_frames_with_nothing_drawn`:
  the demo's messages go, one frame leaves their emoji out, and nothing
  draws after. Their clips close, and a frame of theirs leaves memory and
  the atlas, with no frame drawn. It fails when closed clips' frames are
  retired (they wait for a frame).
- `animated_emoji::tests::emoji_at_rest_on_the_layer_stay_open_while_nothing_draws`:
  emoji on the conversation's animation layer stay open through 10 s with
  nothing drawn and through the next frames, and the closer looks at most
  once in five frames. It fails with a closer that looks at each frame.
- `sticker_player::tests::painting_a_view_closes_no_clip_the_frame_has_not_painted_yet`:
  fails when the paint path closes clips. (In the UI tests every frame
  renders the slices again, so no view is left at rest unpainted.)
- `animated_emoji::tests::a_chat_windows_emoji_at_rest_stay_open_while_the_main_window_draws`
  (the demo chat in its own window; it fails when chat-window draws are
  stamped with the main window's frame count) and
  `a_chat_windows_emoji_close_with_it`. GPUI's test mode draws every
  window the app notifies, so a main window that stays still is tested on
  the player: `sticker_player::tests::each_window_judges_the_clips_it_drew_by_its_own_frames`
  fails the same way.
- `sticker_player::tests`: `a_clip_no_view_draws_closes_with_nothing_drawing`,
  `a_clip_whose_frame_is_on_screen_stays_open`,
  `a_clip_waits_its_full_time_from_the_frame_that_left_it_out`.
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
  after; this branch prints `closed N clips no window shows` and then
  `clips 0 store 0.0 MB` 5 to 7 s after the panel closed.
- `footprint -p` and `vmmap --summary` (MALLOC_SMALL resident): base keeps
  the store that the last trace line shows, plus the canvases; this branch
  should end near its value before the panel opened.
- Then switch to another chat (which closes the clips the main window's
  chat used; `chat-switch-clips.md`) and
  idle 30 s: the difference left is the store held in small blocks, which
  this branch gives back.
- Chat window: open a chat of animated emoji in its own window, maximize it
  over the main window (or minimize the main window), scroll its sticker
  panel, close the panel, and idle 30 s: the trace shows its clips closing
  6 s or so later. Close the window: its clips close the same way.
