# Animated content shares one 60 Hz tick

On a 120 Hz Mac (M1 Pro, release build, `ready-chats` demo with
`QUILL_DEMO_HISTORY_ANIM=video,emoji,stickers`, 1200×620), a chat with one
inline clip drew about 50 window frames a second at about 11% CPU. With three
animated custom emoji next to the clip: about 109 frames a second at about 25%.

## What was wrong

Animated content asked for window frames from two schedulers that did not know
about each other:

- The frame clock (`frame_clock.rs`, origin/main line 89) slept `1 / fps` from
  whenever its last tick fired, so its ticks drifted across the display's
  vsyncs.
- Each playing clip asked for the vsync its next picture is due on
  (`request_video_frame`, origin/main `frame_clock.rs:195`, from
  `VideoPlayer::frame_demand`'s `next_due`).

On a 120 Hz display these land on different vsyncs, and the window draws their
union. A 30 fps clip and 30 fps emoji cost up to 60 full window frames a second.
Telegram Desktop drives every animation from one shared tick
(`Ui::Animations::Manager`), so content at 60 fps or less never costs more than
about 60 repaints a second together.

## What changed

`frame_grid.rs` adds the content grid. On a display refreshing a whole number of
times 60 a second (120, 240 Hz), content shares the display's grid vsyncs
(every other one at 120 Hz). All sources on one display keep one phase. This
only happens while more than one source animates on the display: the clock and
a clip, or two clips. Each source notes itself (`note_playing`) and checks for
the others (`shared`).

- **The clock.** It ticks on grid vsyncs when its rate divides 60 (60, 30, 20,
  15 fps), one content frame after the vsync its content last drew on. After a
  tick off the grid (sharing just began), that is the vsync after the display
  link's latest, because the tick draws on the next frame. Before its first
  tick, it is the latest vsync, where the content that started the clock drew.
  Counting from the latest vsync after an off-grid tick planned a tick for the
  frame that tick was still waiting for: it fired before anything had asked
  again, so the clock stopped. Its restart at 30 fps held a 60 fps sticker's
  frame for 4 vsyncs.
- **Clips.** A clip draws each picture on the next grid vsync, at most one
  display frame later, but only once its pictures have come a whole number of
  grid ticks apart three times in a row (30 or 60 fps, not 24, 25 or 50). Each
  picture is then held exactly as long as before, and the picture shown still
  matches the time it is shown at.
- **Wake timing.** The clock wakes three quarters of a display frame before its
  frame starts, and so does a clip on the grid (`PresentEstimator::request_lead`),
  so a timer up to 6 ms late at 120 Hz still makes its frame. That is as early
  as a wake can come: the lead is measured from the draw's start, which follows
  the display link's call, so the frame before has always started.
- **Lead.** A whole-app frame (`QuillApp::render`) measures the lead from its
  start to its present time. A frame can start well after the display link's
  call (a busy main thread), but never before, so `ClockTiming::sample` keeps
  the longest lead lately seen. It shrinks by a sixteenth of a frame at each
  sample, so a real change still shows, and resets when the display or its
  period changes. Before, one whole-app frame that started more than three
  quarters of a frame late made every later tick miss its vsync and draw a
  frame after the clip's, until the next whole-app frame.
- **Layer redraws.** When a clip's picture redraws an animation layer, its emoji
  draw with it. The clock's next tick then counts one content frame from that
  draw (`later_tick`, `LayerDraws`). A tick's own draw never counts, and the
  due time is not clamped to now, so a late wake is never put off. Only a
  picture of a clip that keeps time with the emoji counts: its pictures come
  a whole number of emoji frames apart, or the other way round, and steadily
  (`frame_clock::clip_frame_keeps_time`, from `Cadence::even_spacing`). A
  24 or 25 fps clip's land between the emoji's grid ticks, and a 12 fps
  clip's on every other one, ten refreshes apart at 120 Hz. Counting those
  held an emoji frame for 6 vsyncs in place of 4.
- **Content time.** While the clock ticks on the grid, the main window's
  play heads read the time of its last tick vsync at or before the frame
  they paint for (`frame_grid::content_time`), not the paint's own time. A
  head starts on whatever paint first shows its frame, so its frame
  boundaries fall on any refresh. Read at paint time, a draw for an
  off-grid clip or a scroll one or two refreshes after a boundary showed
  the next frame early, and the next tick held it again: holds of 1 and 5
  refreshes, or 3 and 5, in place of 4, at three of the four phases a head
  can start on. Now a frame changes only on a tick, so each is held alike
  at any phase. A frame that shows a picture of a clip keeping time with
  the content counts as a tick (`content_at`): the tick it puts off moves
  there, and reading the last tick's time on it would skip a frame.
- **Layers a clip keeps drawing.** When a layer's inline clip has already asked
  for a picture due a display frame or more before the layer's content needs
  its next frame (a 60 fps clip beside 30 fps emoji), the layer asks for no tick
  (`layer_redrawn_by_clip`), and the clock stops if nothing else asked. The
  picture's draw asks again, so the clock comes back when the clip stops or
  slows. Each clip's request in flight is noted for this
  (`frame_clock::video_request`). Before, the tick was put off at every picture
  and never ran, but the clock still woke about 60 times a second.
- **Snap slack.** A picture due up to 0.25 ms after a grid vsync draws on it,
  because AVPlayer's due times are display link vsyncs. One due later
  (FFmpeg's own picture times) is not ready on that vsync, so it draws on the
  next.

Nothing else changes:

- Clock content at other rates (24, 25 fps GIFs) keeps its own rate.
- At 60 Hz every vsync is on the grid.
- At 90, 144 and 165 Hz there is no grid.
- Without the display link's timing (Linux, Windows), behavior is as before.

Motion the frame clock drives at 60 fps (swipe spring-back, the sidebar peek,
the send animation, history effects) ticks on every grid vsync while a clip
plays. That is still 60 a second, now evenly spaced. Anything drawn for input
(scrolling, typing, hover) never goes through the grid.

## Measured (simulation)

`frame_grid_tests.rs` runs the app's scheduling on a simulated clock: `ClockRun`,
`later_tick`, `LayerDraws`, `layer_redrawn_by_clip`, `ClockTiming::sample`,
`PresentEstimator`, `video_request`, and the grid and its sharing. A notified
view draws on the next display frame. Timers fire up to 1 ms late in the first set of runs; in the
second set, up to a tenth of the wait plus 2 ms, or always that late.

Window frames per second at 120 Hz, before → after, and emoji frames skipped in
3 s:

| content | timers ≤ 1 ms late | timers late | emoji skips |
| --- | --- | --- | --- |
| 30 fps emoji + 30 fps clip, separate views | 50 → 30 | 50–51 → 30 | 1–10 → 0 |
| 30 fps emoji + 30 fps clip, one layer | 58 → 30 | 47–50 → 30 | 0 → 0 |
| 60 fps sticker + 30 fps clip | 76 → 60 | 69–75 → 60 | 0–17 → 0 |
| 60 fps sticker + 60 fps clip | 88 → 60 | 87–95 → 60 | 0–17 → 0 |
| 30 fps emoji + 24 fps clip | 47 → 48 | 44–46 → 48–49 | 1–10 → 0 |
| any one source alone | the same | the same | the same |

Fixed in review, before → after:

- A 30 fps clip starting at 12 times beside a 60 fps sticker, in its own view or
  on the sticker's layer: the clock stopped and restarted in 13 of 24 starts,
  skipping a sticker frame in 9 → none.
- One whole-app frame starting 0.9 of a frame late: 30 fps emoji + 30 fps clip
  drew 60 window frames a second from then on → 30; 60 fps sticker + 30 fps
  clip 90 → 60.
- A 60 fps clip with 30 fps emoji on one layer: the clock woke 60 times a
  second → 0, with 60 window frames a second either way.
- A 24 or 25 fps clip on the layer of 30 fps emoji (2026-11-01): emoji held
  2, 4, 4, 6, 4 vsyncs over and over → 4 every time, at four clip phases.
  On #1252 the Mac showed 36 to 45 of about 235 emoji holds at 50 ms,
  against 5 to 11 before the grid.

Clips sharing the grid keep their holds (4 display frames each at 30 fps). With
late timers their holds are steadier than before: 3–5 frames before, 4 after.
A 24 fps clip keeps 5 display frames per picture, alone or beside emoji. Its
few extra draws are emoji frames that were skipped before. At 60 Hz, draws and
pictures land on the same vsyncs as before.

## Not fixed

- **A lone clip's wake is tight.** It still wakes half a frame before its frame
  starts (`pacing::request_policy`), so with timers over 4 ms late at 120 Hz
  some pictures show a display frame late.
- **No grid at 144 or 165 Hz.**
- **A 29.97 fps clip bunches its long holds.** Such a clip needs a 5-vsync hold
  every 8.3 s at 120 Hz. Each time that moves its pictures off the grid's phase,
  it leaves the grid for three pictures, and coming back holds one more picture
  5 vsyncs; the next one moving it back on phase holds 4. Over 20 s in the
  simulation, both have two 5-vsync holds, but on the grid they come 3 pictures
  apart instead of 8.3 s. Coming back only on the grid's phase would avoid it,
  but the clip would then draw apart from the emoji half the time.
- **A tick woken just as a frame is asked for.** A tick off the grid that runs
  after the display link's call but before that frame's draw draws on the
  latest vsync, not the one after. If sharing begins right then, the next tick
  can come one vsync late and hold a frame that much longer.

## The clock on the display's vsyncs (2026-11-01)

The clock ran free outside the grid, 92% of a frame after its last wake. A
60 fps tap effect on a 60 Hz display missed one refresh, painted, then missed
two, in about half of its 3 s plays. A wake a little late landed just after a
frame started, so that frame drew nothing. The next wake came before the frame
after it, with nothing asked since the last draw, and the clock stopped. That
draw started it again at its first rate of 30 fps, whose 33 ms wait passed two
more frames. In nine plays traced on a fast release build, six lost a refresh
and all seven restarts came in play. With the changes below, none of nine did.

- **Ticks.** Off the grid the clock ticks on the display's vsyncs when a whole
  number of them make one content frame (`frame_grid::vsync_tick`), with the
  grid's wake timing. Each wake then sits between two frame starts. A rate no
  vsync fits (24 or 25 fps at 60 Hz, 60 fps at 144 Hz) runs free as before: a
  chain of snapped ticks there would drift away from the rate.
- **Stopping.** A wake with nothing asked no longer stops the clock; a second
  one in a row does (`ClockRun::woke`). On systems without vsync times the
  race remains, a late tick can still miss one refresh, and this keeps it from
  missing three.
- **Restarting.** A clock starts at the rate that asked, not at 30 fps.
- **Play heads.** On a display reporting vsyncs, every head reads the vsync its
  frame shows on (`frame_clock::content_at`), not the paint's wall time, in every
  window. So a late paint moves nothing, and any draw between ticks shows what
  the last tick showed. `grid_slack` stays for displays without vsync times.
- **What a tick asks.** A tick that can skip its draw (`idle_layer_tick`) asks
  about the vsync it draws on (`TickDraw::On`), so it draws only on the refresh
  a frame changes.
  Without vsync times it still asks about the whole next refresh
  (`TickDraw::Within`), and a frame costs one or two draws.

The cost is at 60 Hz beside a clip. There is no grid to share there, and a
clock on vsyncs no longer drifts into phase with a 30 fps clip: when they sit on
opposite refreshes the window draws close to 60 frames a second where the free
clock averaged 43. A chat with a clip, emoji and stickers took 16% more
instructions. Its 30 fps stickers held two refreshes 99.7% of the time, up from
93%, so this stays until a 60 Hz display shares vsyncs. A 24 fps sticker beside
30 fps content now holds two or four refreshes rather than two, three or four: a
clip's draw between ticks no longer shows it early.
