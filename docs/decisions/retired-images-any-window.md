# Retired pictures leave whichever window draws

Status: done (Linux tests); the Mac numbers are for the lead to measure.
Follows `sticker-frame-leak.md` and `owned-images.md`.

## Symptom

Measured on a Mac with a release demo build of main 04644b4a3, ready-chats
demo, a chat of animated stickers open in its own window and playing:

| Main window | Footprint |
| --- | --- |
| Visible (also when covered by the chat window) | 146 to 161 MB, flat |
| Minimized | 373 MB at 10 s, 1,388 MB at 50 s, about 25 MB/s and unbounded; about 640 MB each of IOAccelerator (atlas textures) and App-Specific Tag 1 |
| Restored | 137 MB within a second of the main window drawing |
| Closed (its close button), chat window open | 367 MB at 10 s, 1,353 MB at 50 s |
| Then the chat window closed too (no window) | 697 MB, kept: IOAccelerator 4 MB, App-Specific Tag 1 657 MB |

A counter in the sweep, with the main window not drawing: `frame()` froze
at 155 while the retired list grew by about 54 sticker frames a second of
about 250 KB each: 571 images (143 MB) at 10.5 s, 2,597 (649 MB) at
48.1 s. The first main frames afterwards emptied it to 2.

## Cause

Images Quill makes itself (sticker and video frames, blurred previews,
`OwnedImage`) go to `image_budget::retire_all` when their cache lets go of
them, and wait there until no frame can show them. Two things tied that
wait to the main window:

1. Only `QuillApp::render` swept (`image_budget::sweep`). A chat window
   never did, nor did any other window.
2. "No frame can show it" was judged by the main window's frame count
   (`retired_may_show`: `at + 1 >= frame()`). With the main window not
   drawing, the count stood still, so every frame a chat window retired
   looked as if the main window's last frame might show it.

macOS stops drawing a minimized or fully covered window, and a pending
change stops the main window drawing whenever a chat window covers it.
Closing the main window keeps Quill running while a chat window is open
(the chat window's `QuillShell` holds `QuillApp`). With no window open,
the retired list was never swept again; the windows' atlases went with
them, but the list's `Arc`s kept every picture's pixels.

`App::drop_image(image, Some(window))` takes the image out of every
window's atlas (the window being drawn is passed in because it is out of
`App.windows` then), so where the sweep runs does not matter, only when.

## Fix

`src/ui/image_budget/retired.rs` holds the retired list and the sweep:

- A sweep runs between frames and asks every open window what its latest
  frame can show. A frame shows only images that existed when it was done
  (image ids only grow), and a window that stops drawing keeps its last
  frame.
  - The main window: what `QuillApp` drew in its latest frame, and what the
    slices it replayed last rendered, both as of that frame however long
    ago it was drawn (`MainFrame`). The main window's frame count is still
    `QuillApp`'s renders, so its slices stay replayable while it does not
    draw. Each frame gets a bound on the image ids it painted once it is
    done (`close_main_frame`): before, a slice got that bound only when the
    next main frame started, and a frame of `QuillApp` itself had none.
  - Every other window (chat windows, Picture-in-Picture, call windows):
    GPUI's count of the frames it began (`Window::frame_number`). A sweep
    notes each window's latest frame and the next image id when it first
    sees that frame. An image retired after sweep `s` may show in that
    frame if the frame was first seen by sweep `s + 1` and the image is
    older than it. A frame first seen later began after the retirement.
  - A closed window shows nothing; with no window open, everything retired
    that nothing else holds goes.
- Sweeps run as the main window starts a frame (`begin_main_frame`, before
  the frame counts, the same moment as before), and in a task
  (`start_sweeper`, started by `QuillApp::new_with_demo`) woken by a
  retirement, a chat window's frame start or a window closing. The task
  runs between frames, and an app with no window open still runs it.

Rejected: calling `begin_frame` and `sweep` from every window's render.
Two frame starts per tick moved the main window's count on without it:
its cached slices would count as gone, and a picture only a slice replays
would leave the atlas while the main window still showed it, a blank on
its next present or a reload when it comes back.

### Windows that replay cached views

The rule for windows other than the main one takes a newer frame to
redraw everything. The 1:1 call and video chat windows replay cached
views (`calls/call_video.rs` 307, 319, 326; `group_call_window.rs` 272,
279), so a newer frame can show what an older one painted. Their video
is still safe: a `VideoFrame` pulls its picture when its view renders
(`video_player/element.rs` 115), and the player lets go of (retires) the
picture it showed only on such a pull, so a video frame is retired in a
frame where its view rendered again, never while a replay can still show
it. Live call video keeps the picture before the one shown for one more
pull and drops its own pictures. The backdrop under the 1:1 call's views
is drawn by the window's root, which renders every frame. A picture
retired while a cached view in such a window still replays it, without
that view being redrawn, would leave too soon here, as it left after two
main frames before; none is known.

### What a window that stops drawing keeps

A window that stops drawing (minimized, covered, hidden) keeps every
retired picture that existed when its last frame was done, not only what
it painted, until it draws again or closes. Example: a chat window
minimized while the main window switches chats. Stopping the old chat's
stickers retires every clip's buffered frames, which before left within
two main frames and now stay until the chat window draws or closes. This
is bounded: a picture made after that frame never counts, so the set
cannot grow while the window stays as it is. Keeping only what the window
painted needs to know what it painted. Paints cannot be observed, and the
real atlases (Metal, wgpu, DirectX) answer "does this window's atlas hold
this image" only in test builds; asking them would mean changing all
three vendored backends. Before, those pictures left after two main
frames even when the idle window's last frame showed them, so the window
could show a missing tile when presented again.

## Cost

One task wake and one sweep after each frame that retires something, and
after a chat window's frame while anything waits. The sticker player
hands over an empty list for every frame it fetches; that wakes nothing.
While the main window draws, its frames also sweep, as before. A sweep
leases each open window other than the main one to read its frame count
and makes one pass over the retired list, which now holds a frame or two
of pictures.

## Tests

- `image_budget::retired::tests::windows::a_chat_windows_frames_leave_while_the_main_window_does_not_draw`:
  a main window stand-in (draws like `QuillApp`: a picture of its own, a
  slice that replays an old render, a slice that renders every frame)
  stops drawing; a chat window stand-in paints a new picture each frame
  and retires the one before. After 61 frames at most 2 of its pictures
  are held and at most 2 are in its atlas; the main window's own picture
  and the one only its replaying slice shows stay in its atlas until it
  draws again, then leave. Checked against three wrong versions: with no
  sweep outside the main window's frames (the old code) all 61 stay; with
  each sweep outside them counted as a main frame (`begin_frame` from any
  window) the main window's pictures leave while it still shows them; with
  slice bounds set only as the next main frame starts (the old
  `begin_frame`), the slice that rendered in the main window's last frame
  keeps all 61.
- `..::a_picture_a_chat_window_shows_stays_until_that_window_draws_again`:
  through the real sweep, a chat window stand-in lets go of the picture its
  frame on screen shows without drawing. The picture stays in its atlas
  through three main window frames, and leaves once the chat window draws
  without it. Checked: it fails with a sweep that reads no window but the
  main one (the picture goes at once) and with a constant frame count for
  other windows (it never goes). The old code dropped such a picture
  after two main frames, which this test also rejects.
- `..::retired_pictures_leave_with_the_windows_that_showed_them`: the main
  window closes while the chat window plays (its picture leaves, the chat
  window stays bounded), then the chat window closes: every picture goes,
  and so does one retired with no window open. It fails on the old code.
- `image_budget::retired::tests`: `an_image_retired_twice_still_leaves`,
  `frames_retired_while_a_slice_replays_leave_after_the_next_frame`,
  `a_slice_render_covers_what_was_made_until_its_frame_was_done`,
  `a_window_that_stops_drawing_keeps_only_what_its_last_frame_may_show`.

## Not tested here

- The Mac footprint. Expected after the fix: flat at 146 to 161 MB with
  the main window minimized, covered or closed; back near the idle
  footprint after closing every window; no blank or reloading picture in
  the main window when it comes back.
- GPUI's test mode draws every window the app notifies, so the tests use
  stand-in views for "the main window does not draw" rather than the demo
  app.
- Windows and Linux stop drawing hidden or minimized windows too (the
  "Run in background" close hides the main window there); the sweep does
  not depend on any platform drawing the main window.
- Reopening from the Dock: Quill sets no `on_reopen` handler, so with no
  window open a Dock click opens nothing (unchanged here). With this fix
  the retired list is empty by then.
