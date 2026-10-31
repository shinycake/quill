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
window), with the pass it last rendered in there and whether that window's last sweep found it in view
(`src/ui/inline_players/windows.rs`).

- Pass counts are per window and list (history, inline results, GIFs). A window's pass takes only its own
  entry off a clip whose row it stopped rendering. The player goes when no window shows the clip.
- A window's sweep judges only the clips that window shows, and marks only their paint counts. A clip is
  paused when no window has it in view, so one window cannot pause what the other plays.
- Each window runs the frame start (`QuillApp::inline_videos_frame_start`) with its own state in place:
  the main window from its render, a chat window from its view's render. A frame with no history pass, a
  viewer over the window's chat, or a panel closed there takes that window's entries off. The chat window's
  close removes its entries (`InlineVideos::forget_window`).
- A sleeping clip that wakes redraws the history views of every window showing it. Before, it redrew the
  history view of whichever window passed last.

With one window every rule is the same as before: one entry per clip, the same pass numbers, the same
sweep. Opening the viewer still clears pending play-on-demand requests, through `leave_window`. A clip
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

Unit tests in `windows.rs` cover the pass and sweep rules for one window, two windows, and a clip both show.

## Expected on the Mac

Use the release demo build and the same setup: the round video in a chat window, and the main window
visible on another chat. Expect about 26% CPU (as with only stickers playing), both windows at their
content's rate (about 29-30 frames/s, not 60), no reopens after the first open, and the round video's
picture playing. Minimizing the main window should change nothing.

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
- A minimized chat window keeps its clips' players until it draws again, as the main window does when
  minimized. While another Quill window is active, those muted loops stay in the playing state.
