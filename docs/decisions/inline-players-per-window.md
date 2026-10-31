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
  another Quill window is presented. So has the main window while the active chat window hides it whole
  (`chat_window::cover`, #1264), which macOS does not report as hidden: it draws nothing then. A minimized
  chat window, or a main window that is minimized, covered by another app or hidden by its chat window,
  pauses its clips at once and lets them sleep after `RELEASE_AFTER` (2 s), as clips scrolled away do.
  When it is presented again, a recheck plays what it last had in view; a sleeping clip gets a new player,
  which redraws that window's history. With no window presented, or with one window, nothing changes from
  before.
- Each window runs the frame start (`QuillApp::inline_videos_frame_start`) with its own state in place:
  the main window from its render, a chat window from its view's render. A frame with no history pass, a
  viewer over the window's chat, or a panel closed there takes that window's entries off. The chat window's
  close removes its entries (`InlineVideos::forget_window`).
- Whether the GIF panel is open is read from that window's own media panel (`pickers.media_panel`, open on
  the GIFs tab). The session's `stickers.gifs.open` is one flag for every window. When the main window
  closed its GIF panel, the flag made each chat window frame take its GIF players off and its panel open
  them again.
- A play-on-demand request (a click with autoplay off) records the window it was asked in, carried through
  a pending download (`PlayTarget::Inline::window`). The media viewer drops only the requests of its own
  window. Before, a viewer open in the main window dropped a request from a chat window before its player
  opened, so the click did nothing. The stories viewer is still one for the app: while it is open, every
  window's frame start drops its clips and requests, and no window shows a clip inline
  (`src/ui/inline_video.rs:241,268`). That predates this change.
- A click parked for a download in a chat window that closes before the file lands is dropped with the
  window (`drop_play_intent_in_window`, from `forget_chat_window`). Before, the request it left once the
  file landed named the closed window, so nothing dropped it, and the clip started with sound the next
  time its chat showed in any window.
- A round video's sound ending redraws the history of the window it plays in (`history_redraw_target`),
  not the main window's conversation slice.
- A sleeping clip that wakes redraws the history views of every window showing it. Before, it redrew the
  history view of whichever window passed last.
- Behind another app, only a window's own history pass wakes a sleeping clip; a recheck or the release
  timer does not. Otherwise, with a chat window minimized and its clips asleep, Cmd-H (or another app
  covering the main window whole) left no window presented, which counts as nothing out of sight, and the
  recheck opened a decoder for each of those clips at every app switch. Once the app is active again,
  the window's next pass wakes what it has in view.

With one window every rule is the same as before: one entry per clip, the same pass numbers, the same
sweep. Opening the viewer still clears the window's pending play-on-demand requests, through
`leave_window`. A clip shown in both windows (the same GIF in both GIF panels) keeps one player. It stays
while either window shows it and plays while either has it in view.

## A clip froze when its window lost focus

Traced on the Mac at d6e69c4d6: a clip stopped as soon as its own window lost focus to another Quill window,
and stayed stopped while still playing, in view, not paused and not asleep. With the clip alone in the chat
window: 22.1 frames/s with the chat window focused, 0 once the main window was clicked, 21.3 once the chat
window was clicked again. The same happened to a clip in the main window when a chat window took focus.

`request_video_frame` (`src/ui/frame_clock.rs`) asked for the next picture's frame only while the clip's own
window was active (`window.is_window_active()`), unless the clip had sound. The rest of the app counts any
Quill window as active (`QuillApp::render`, the animation clock), so stickers and emoji kept animating in
the unfocused window and kept redrawing a clip next to them. A clip alone had its last asks refused, and
nothing drew its window again.

- `request_video_frame` now asks while any Quill window is active, from the app's gate (`AppGate`, a GPUI
  global that `QuillApp::follow_windows` sets with `frame.window_active`). The main window is left out while
  the active chat window hides it whole (`chat_window::cover`), so a covered main window still draws
  nothing. Behind another app nothing changes: only clips with sound, and Picture-in-Picture, ask then.
- GPUI caps an unfocused window's next-frame callbacks at `WindowOptions::inactive_frame_interval`, by
  default 33,333 µs. That is almost exactly two 60 Hz vsyncs, so a frame two vsyncs after the last one
  often measured a hair under it, was skipped, and slipped a whole vsync, to about 50 ms. The Mac counted
  6.2-8.2 such skips a second, 31-33.3 ms after the last frame. The main and chat windows now set it to
  `window_frame::INACTIVE_FRAME_INTERVAL`, 30 ms, just under two 60 Hz vsyncs
  (`window_frame::window_options` and `open_chat_window`). Picture-in-Picture keeps `None`.

Measured on the Mac at 60 Hz, unfocused window, with the app-wide gate in place:

| Case | frames/s, 33.3 ms cap | frames/s, 30 ms cap | CPU, 33.3 → 30 ms |
|---|---|---|---|
| Clip in the chat window, main window focused | 17.4 | 22.8 | 12.4% → 18.0% |
| Clip in the main window, chat window focused | 16.4-17.4 | 22.1-22.5 | 9.2-11.0% → 11.6-12.8% |
| Everything animating, both windows | 22.8 + 23.5 | 28.8 + 29.5 | 28.6% → 30.6% |

The focused window's numbers did not change. The CPU rises because the frames are now drawn.

At 120 Hz, from GPUI's throttle (`gpui-pre` `window.rs`): an unfocused window's frame with next-frame
callbacks is skipped while less than the interval has passed since the last frame that went through, and
retried at the next vsync. Vsyncs come every 8.3 ms, so with 30 ms the third vsync (25 ms) is held and the
fourth (33.3 ms) goes through with 3.3 ms to spare: an unfocused window draws every fourth vsync, about
30 frames/s. GPUI's default falls exactly on the fourth vsync, so jitter slipped some of those frames to
the fifth (41.7 ms). A vsync with nothing asked also goes through and resets the count (`last_frame_time`),
so a clip that asks between vsyncs (`SleepUntil`) gets its frame on the fourth vsync after the one before
its ask: at most 33.3 ms late, the same bound as the second vsync at 60 Hz, and 3.3 ms of margin at both
rates. The focused window is not throttled at either rate.

The app's gate (`frame.window_active`) also had a gap, found in the code and not traced on the Mac: the
main window's activation observer set it from the main window alone, and only the main window's render
added the chat windows. A main window that does not draw (minimized while focused, so a chat window takes
focus) left the gate off: that chat window's muted loops paused and its stickers held still. The gate is now
recomputed at each activation change of any Quill window (`QuillApp::follow_windows`, from the main
window's observer, a chat window's `placed` observer, a chat window's render that finds its activation
changed, and the main window's render), and when a chat window closes (`forget_chat_window`). The close
needs its own call: GPUI drops the window (`gpui-pre` `app.rs`) before the platform reports it inactive,
so its observer never runs, and a main window that is minimized or hidden to the tray draws nothing.
Without it the gate stayed on after the active chat window closed, with the user in other apps:
`sync_presence` kept the account online, `poll_background` counted the app active and muted the
background accounts' sounds, and muted clips kept playing.

Other effects of the change:

- `AppGate::animates` applies to every window, so a call's or a live stream's video in its own window
  asks for frames while the main window is active too. That matches the animation clock, which already
  ran for any active Quill window.
- The 30 ms interval also applies to a single window behind another app. A clip with sound there now
  gets about 30 frames/s instead of 17-23.
- At 100 Hz, vsyncs come every 10 ms and 30 ms falls exactly on the third, so jitter lets some frames
  through at 30 ms and holds others to the fourth (40 ms). GPUI's default always waits for the fourth, so
  this is no worse than the default.
- macOS does not report one window of an app covering another of the same app. A chat window hidden
  whole behind the active main window, or behind another chat window, is still presented, so it now
  draws its clips at the inactive rate where no one sees them. Only the main window under its chat
  window is handled (`chat_window::cover`). This is a known follow-up, not a regression against main:
  before, such a chat window's clips froze instead.

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

The same file has more tests. Each fails with its fix reverted:

- `a_minimized_chat_windows_clips_pause_then_sleep_while_the_main_window_draws` and
  `a_minimized_main_windows_clips_sleep_and_play_again_once_it_shows` hide one window with the test
  platform's visibility while the other draws. They advance the simulated clock by `RELEASE_AFTER` and
  show the window again. Without the visibility rule: `(11, 611) pauses out of sight`.
- `a_main_window_its_chat_window_hides_pauses_its_clips_till_it_shows` opens a chat window over all of the
  main window, then moves it off. Without the cover counting: `(11, 611) pauses out of sight`.
- `a_chat_windows_clips_keep_their_frames_while_the_main_window_has_focus` and
  `a_main_windows_clips_keep_their_frames_while_a_chat_window_has_focus` move focus between the windows
  and count the clips' paints over a second of the simulated clock, running the window's next-frame
  callbacks every 100 ms. With the own-window gate: `painted [3, 3] times in a second` for the chat
  window's clips once the main window has focus, and `[1, 1]` for the main window's clips once a chat
  window has. The test platform does not throttle unfocused windows, so the 30 ms interval is covered by
  the reasoning above and the Mac's numbers, not by a test.
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

Then move focus: click the main window, then the chat window, then the main window again. The chat
window's clip should keep about its focused rate (22-23 frames/s alone) in every step, and with stickers
and emoji animating too both windows should stay at about 29 frames/s. The same for a clip in the main
window while a chat window has focus. Worth one run at 120 Hz: an unfocused window should draw at about
30 frames/s. With the main window focused, minimize it with Cmd-M: the chat window's clip and stickers
should keep animating.

Then minimize the chat window with the main window in front. Its clips should pause at once and release
their decoders within about 2 s, and CPU should fall to what the main window alone costs. When the window
is restored, they should play again within a frame or two. Minimizing or covering the main window instead
should do the same for the main window's clips while the chat window's play on, and so should a chat
window dragged over all of the main window. Closing the GIF panel in one window should not change CPU or
the GIFs playing in another window's panel.

## Not changed

- `image_budget` counts frames of the main window only (`FRAME`, `begin_frame` in `QuillApp::render`), so
  how long a retired image may still show is judged by main window frames for every window. #1267
  (merged) covers it.
- The sticker player's per-window leave is in PR #1268.
- Video userpics are kept per surface (`Surface::History(window)`), and message effects keep the badges of
  chats shown in other windows. Neither has this problem.
- A true `image/gif` file plays one at a time app-wide (`playing_animation`). Selecting a chat in the main
  window (`chat_list/mod.rs:171`) stops a GIF file playing in a chat window. That stop happens once; it
  does not repeat every frame.
- Paints are counted per player, not per window (`VideoPlayer::shown_count`). Counting them per window
  would need the window in `video_frame`'s paint, outside the inline players, and would only change which
  window a clip in view is credited to.
