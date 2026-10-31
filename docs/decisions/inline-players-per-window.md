# Inline players per window

A round video in a chat open in its own window, with the main window showing another chat, cost a full
core: Quill at 96% CPU (26% with only stickers playing), both windows drawing at about 60 frames/s
instead of about 29, and the round video never got past its grey placeholder. A counter showed the inline
players dropped and reopened about 57 times a second. Minimizing the main window (or covering it so it
stopped drawing) brought it back to 24-25% and the picture showed.

## What was wrong

`inline_players::InlineVideos` holds one muted, looping player per clip on screen for every window, but
it kept one history pass count for all of them (`State::render`, `src/ui/inline_players.rs:447` on main).
A slot survived a pass only if its row rendered in the previous one (`survives`, line 328). So each main
window pass counted against the chat window's clips:

1. The chat window's pass N renders the round video: its slot is seen in pass N.
2. The main window's pass N+1 does not render it (another chat). The slot survives (N+1 is the next pass),
   but the sweep after that pass (`sweep`, line 488) judged every history slot, found the round video not
   painted by the main window, and paused it.
3. The chat window's pass N+2 drops it (seen in N, not in N+1) and the row opens a new player at once.

Every chat window frame dropped and reopened every clip it showed. The new player was still opening when
the next frame dropped it, so it never showed a picture, and an opening player asks for a frame on every
refresh. The paint marks the sweep reads (`shown_mark`, line 474) were also reset by every window's pass,
and the main window's frame start dropped every history clip when no pass of any window had run since
its last frame (line 638). The main window's render also closed every player while its viewer was open
(`src/ui/app_render/mod.rs:93`) and closed the inline results' and GIF panel's players whenever its own
panel was closed (lines 98, 101), even when a chat window showed them.

## What changed

Each clip now records the windows that show it (`None` for the main window, the chat's id for a chat
window), with the pass it last rendered in there, the player's paint count when that window's pass began,
and whether that window's last sweep found it in view (`src/ui/inline_players/windows.rs`).

- Pass counts are per window and list (history, inline results, GIFs). A window's pass takes only its own
  entry off a clip whose row it stopped rendering. The player goes when no window shows the clip.
- A window's sweep judges only the clips that window shows, each by the paints since that window's own
  mark. One window's pass no longer resets the mark another window's sweep reads. Paints are still counted
  per player, not per window, so a mark sees paints in any window; that can only call a clip in view that
  another window has in view. A clip is paused when no window has it in view, so one window cannot pause
  what the other plays.
- A window the platform does not present (`Window::is_visible`, watched with
  `observe_window_visibility` from its first frame until it closes) has none of its clips in view while
  another Quill window is presented. A minimized chat window, or a main window that is covered or
  minimized, pauses its clips at once and lets them sleep after `RELEASE_AFTER` (2 s), as clips scrolled
  away do. When it is presented again, a recheck plays what it last had in view; a sleeping clip gets a new
  player, which redraws that window's history. With no window presented, or with one window, nothing
  changes from before.
- Each window runs the frame start (`QuillApp::inline_videos_frame_start`) with its own state in place:
  the main window from its render, a chat window from its view's render. A frame with no history pass, a
  viewer over the window's chat, or a panel closed there takes that window's entries off. The chat window's
  close removes its entries (`InlineVideos::forget_window`).
- Whether the GIF panel is open is read from that window's own media panel (`pickers.media_panel`, open on
  the GIFs tab). The session's `stickers.gifs.open` is one flag for every window. When the main window
  closed its GIF panel, the flag made each chat window frame take its GIF players off and its panel open
  them again.
- A play-on-demand request (a click with autoplay off) records the window it was asked in, carried through
  a pending download (`PlayTarget::Inline::window`). A viewer drops only the requests of its own window.
  Before, a viewer open in the main window dropped a request from a chat window before its player opened,
  so the click did nothing.
- A round video's sound ending redraws the history of the window it plays in (`history_redraw_target`),
  not the main window's conversation slice.
- A sleeping clip that wakes redraws the history views of every window showing it. Before, it redrew the
  history view of whichever window passed last.

With one window every rule is the same as before: one entry per clip, the same pass numbers, the same
sweep. Opening the viewer still clears the window's pending play-on-demand requests, through
`leave_window`. A clip
shown in both windows (the same GIF in both GIF panels) keeps one player. It stays while either window
shows it and plays while either has it in view.

## Measured

`a_chat_windows_clips_keep_playing_while_the_main_window_draws` (`src/ui/video_player/tests/
inline_windows.rs`, scripted players): chat 11 in its own window shows the round video and the video
tile while the main window shows chat 12, and each window draws 8 frames in turn.

- Before the change: `players opened again over 8 frames of each window: left 16, right 0`. That is both
  clips reopened on every chat window frame.
- With the pass fix but the shared paint marks and sweep: no reopens, but `play / pause churn: ["play 1",
  "pause", ...]`, 32 calls.
- After: no backend opened or dropped. The same players keep playing in view with a picture, and closing
  the window releases them.

The same file has four more tests. Each fails with its fix reverted:

- `a_minimized_chat_windows_clips_pause_then_sleep_while_the_main_window_draws` and
  `a_covered_main_windows_clips_sleep_and_play_again_once_it_shows` hide one window with the test
  platform's visibility while the other draws. They advance the simulated clock by `RELEASE_AFTER` and
  show the window again. Without the visibility rule: `(11, 611) pauses out of sight`.
- `a_viewer_in_the_main_window_leaves_a_click_in_a_chat_window_alone`, with autoplay off. Without the
  window on the request: `the click plays it`.
- `a_gif_panel_closed_in_the_main_window_leaves_the_chat_windows_open`. With the session's GIF flag:
  `GIF panel players opened again over 8 frames of each window: left 40, right 0`.

Unit tests in `windows.rs` cover the pass and sweep rules for one window, two windows, and a clip both
show. They also cover per-window marks, and which windows count as out of sight (none with one window or
with every window hidden).

## Expected on the Mac

Use the release demo build and the same setup: the round video in a chat window, and the main window
visible on another chat. Expect about 26% CPU (as with only stickers playing), both windows at their
content's rate (about 29-30 frames/s, not 60), no reopens after the first open, and the round video's
picture playing.

Then minimize the chat window with the main window in front. Its clips should pause at once and release
their decoders within about 2 s, and CPU should fall to what the main window alone costs. When the window
is restored, they should play again within a frame or two. Minimizing or covering the main window instead
should do the same for the main window's clips while the chat window's play on. Closing the GIF panel in
one window should not change CPU or the GIFs playing in another window's panel.

## Not changed

- `image_budget` counts frames of the main window only (`FRAME`, `begin_frame` in `QuillApp::render`), so
  how long a retired image may still show is judged by main window frames for every window. PR #1267
  covers it.
- The sticker player's per-window leave is in PR #1268.
- Video userpics are kept per surface (`Surface::History(window)`), and message effects keep the badges of
  chats shown in other windows. Neither has this problem.
- A true `image/gif` file plays one at a time app-wide (`playing_animation`). Selecting a chat in the main
  window (`chat_list/mod.rs:171`) stops a GIF file playing in a chat window. That stop happens once; it
  does not repeat every frame.
- Paints are counted per player, not per window (`VideoPlayer::shown_count`). Counting them per window
  would need the window in `video_frame`'s paint, outside the inline players, and would only change which
  window a clip in view is credited to.
