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
  15 fps), one content frame after the vsync its last tick drew on.
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
- **Layer redraws.** When a clip's picture redraws an animation layer, its emoji
  draw with it. The clock's next tick then counts one content frame from that
  draw (`later_tick`, `LayerDraws`). A tick's own draw never counts, and the
  due time is not clamped to now, so a late wake is never put off.
- **Snap slack.** A picture due up to 0.25 ms after a grid vsync draws on it,
  because AVPlayer's due times are display link vsyncs. One due later
  (FFmpeg's own picture times) is not ready on that vsync, so it draws on the
  next.

Nothing else changes:

- A source animating alone keeps exactly its old timing.
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
`later_tick`, `LayerDraws`, `ClockTiming::new`, `PresentEstimator`,
`video_request`, and the grid and its sharing. A notified view draws on the next
display frame. Timers fire up to 1 ms late in the first set of runs; in the
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

Clips sharing the grid keep their holds (4 display frames each at 30 fps). With
late timers their holds are steadier than before: 3–5 frames before, 4 after.
A 24 fps clip keeps 5 display frames per picture, alone or beside emoji. Its
few extra draws are emoji frames that were skipped before. At 60 Hz, draws and
pictures land on the same vsyncs as before.

## Not fixed

- **The clock alone still drifts.** Each tick sleeps `1 / fps` after the last
  fired, so with late timers a lone 30 fps emoji skips 2 to 3 frames a second
  (6–10 in 3 s in the simulation). Ticking on the grid when alone (for rates
  that divide 60) would fix it.
- **The clock can stop early.** It stops when a tick fires before the previous
  tick's frame drew, because the tick period (0.92 of a frame) is shorter than
  the time from tick to draw. This hits 60 fps content on a 60 Hz display and
  120 fps stickers at 120 Hz. In the simulation a lone 60 fps sticker at 60 Hz
  draws about 26 of 60 frames.
- **A lone clip's wake is tight.** It still wakes half a frame before its frame
  starts (`pacing::request_policy`), so with timers over 4 ms late at 120 Hz
  some pictures show a display frame late.
- **No grid at 144 or 165 Hz.**
