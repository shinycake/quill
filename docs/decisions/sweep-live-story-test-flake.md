# Live story header test no longer depends on machine load

`ui::story_viewer::live_tests::the_header_is_the_live_controller` failed when the machine was loaded.

## Cause
The test asserts a playing live story draws no stalled overlay or spinner. The demo feed pushes one frame stamped with the
real `Instant` it was made at (the 30 fps repeat runs on the test executor's simulated timer, so it never fires). The live
player calls a feed stalled when the real time since its newest frame passes `STALL_AFTER` (1.5 s). With the CPUs
oversubscribed, more than 1.5 s of real time passed between the setup and the assertion, the picture turned
Buffering, and `live-stalled` with `live-spinner` appeared. The overlay's own 500 ms spinner delay was never the problem:
`LiveOverlayClock` already reads `background_executor().now()`, which the test executor simulates.

Production is correct: frames really arrive over the network, so real time is the right clock. Nothing there changed.

## Change
`open_live` (shared by every live story test) now calls `hold_the_feed_clock`, which reads the player's clock as the
frame's own arrival time with the existing `set_live_clock` test hook. The picture plays until a test moves the clock, so
no assertion waits on wall-clock time. What the test asserts is unchanged.

## Other tests with the same pattern
The other tests that build on a live feed already pin or inject the clock (`group_call_stream_tests`,
`video_player::live_view_tests`) and passed repeatedly under the same load. No other test asserts on a real-time delay.

## Verified
30 runs of the test with 32 busy loops on 8 cores: 0 of 30 passed before, 30 of 30 after. The whole `live_tests` module also
passed under load.
