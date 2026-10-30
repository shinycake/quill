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
A 50 fps clip and a 60 fps sticker cost about 84. Telegram Desktop drives every
animation from one shared tick (`Ui::Animations::Manager`), so content at
60 fps or less never costs more than about 60 repaints a second together.

## What changed

`frame_grid.rs` adds the content grid. On a display refreshing a whole number of
times 60 a second (120, 240 Hz), content draws only on every `step`-th vsync of
the display, anchored on the display link's present times. All content shares
one phase per display.

- The frame clock wakes for the grid vsync one content frame after the vsync
  its last tick drew on. It wakes half a vsync before that frame starts drawing,
  the same way a clip wakes (`frame_clock::tick_wait`).
- A clip picture due on another vsync draws on the next grid vsync, at most one
  display frame (8.3 ms) late (`PresentEstimator::on_grid`). The picture still
  matches the time it is shown at, so A/V sync and playback speed do not change.
  A clip faster than the grid (more than 60 fps) keeps its own vsyncs.
- When a clip's picture redraws an animation layer, its emoji draw with it. The
  clock's next tick then counts from that draw instead of drawing the window
  again a grid tick later (`later_tick`, `note_layer_drawn`).

Some cases do not change:

- At 60 Hz every vsync is on the grid.
- At 90, 144 and 165 Hz there is no grid, because a grid of those vsyncs would
  draw 30 and 60 fps content more often than before.
- Without the display link's timing (Linux, Windows), behavior is as before.
- User-driven motion (scrolling, springs, the caret) never goes through the
  grid.
- Content faster than the grid, such as stickers on the 120 Hz fine grid, keeps
  its own rate.

## Measured (simulation)

`frame_grid_tests.rs` runs the app's own scheduling code on a simulated clock.
Timers fire up to 1 ms late, and a notified view draws on the next display
frame. Window frames drawn in one second at 120 Hz:

| content | before | after |
| --- | --- | --- |
| 60 fps sticker + 50 fps clip | 84 | 60 |
| 30 fps emoji + 30 fps clip, separate views | 50 | 30 |
| 30 fps emoji + 30 fps clip, one layer | 58 | 30 |
| 60 fps sticker + 30 fps clip | 76 | 60 |
| 30 fps emoji + 25 fps clip | 50 | 45 |
| clip alone (25, 30, 50 fps), emoji alone (30 fps) | 25, 30, 50, 30 | the same |
| 60 Hz display: 30 emoji + 30 clip, 60 sticker + 50 clip | 39, 53 | the same, on the same vsyncs |

Every emoji and sticker frame shows once, in order. Before the change, a 30 fps
emoji now and then skipped a frame as the clock's timer drifted across vsyncs.
Every clip picture shows within one grid tick of its time.

## Not fixed

The clock stops when a tick fires before the previous tick's frame drew. This
happens with 60 fps content on a 60 Hz display, and with 120 fps stickers on a
120 Hz display. The tick period (0.92 of a frame) is shorter than the time from
tick to draw, so the clock sees no demand and restarts at 30 fps. The simulation
draws about 26 of 60 frames for a lone 60 fps sticker at 60 Hz. A fix would
phase-lock those ticks too, or stop the clock only after two empty ticks.
