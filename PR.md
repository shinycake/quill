Sticker timing tests fail with a clear cause instead of hanging or flaking on leftover state

What was wrong

The second review of the sticker timing work (review.md on claude/review-sticker-timing-2) named one blocking item and two test defects:
- Blocking: the branch conflicted with main in src/ui/frame_clock.rs and src/ui/chat_window/cover_tests.rs.
- The frame grid sim kept the clip asks and answers (VIDEO_ASKS, ANSWERED) between sims on one test thread. A sim run after one with a 60 fps clip held emoji frames for 2 and 6 refreshes instead of 4.
- video_stickers_open_when_no_opener_thread_starts blocked forever on the waker without its fix, so it failed only through CI's timeout.
On top of that, sweep-all's bin tests did not compile on Linux or Windows. The merge cleanup in 7227840e removed the Warmed import as unused, but a test that only builds off macOS still uses it.

What changed

- Blocking conflict: sweep-all already merges main the way the review asks. The `idle_layer_tick` skip sits at the top of main's `for id in views` loop, and cover_tests keeps both sides' tests. No commit was needed. cover_tests, frame_grid and frame_clock pass (see below).
- Warmed import: opening_a_player_starts_the_load imports it itself, so neither platform warns.
- Frame grid sim: Sim::new clears both thread locals through a test-only forget_video_requests. The new test a_sim_after_one_with_a_clip_that_keeps_time_holds_emoji_frames_alike runs a 60 fps clip sim, then a 24 fps clip at head phase 1, and expects holds of exactly 4. Without the fix it fails with holds of 2 and 6.
- No opener thread: the worker loop's job step is now worker::work. The test starts a worker that does not run, takes the worker's next job itself and runs it through worker::work. With workers_decode forced to false, the test fails at once with "no thread takes the clip to decode it" instead of hanging. It uses no sleep, timeout or wall clock.

What ran

- cargo fmt --check, and clippy -D warnings for --features demo-capture --all-targets and --no-default-features --all-targets: clean.
- cargo test --features demo-capture -- sticker_playback::player:: sticker_player frame_grid frame_clock cover_tests anim_layer layer_draws video_player: lib 31 passed and 1 ignored. App 251 passed and 1 failed (see below).
- Each new test was checked with its fix taken out, and it fails.

What did not run

The full test suite, the Windows target check and anything on a real display or on macOS. The changes are platform independent test and refactor code.

What stays open

- ui::anim_layer::layer_tick_tests::a_sticker_tick_renders_the_layers_alone fails on Linux when it runs with the other tests ("it plays: {"(window frame)": 1, "animation layer": 3}"). It passes alone. It fails in the same way on sweep-all plus only the import fix, three runs out of three, so this branch did not cause it. It probably waits on real sticker worker threads while the test clock is simulated. It is not one of the review's findings.
- frames_of in src/sticker_playback/player/tests.rs still polls against a 20 s wall clock deadline. That was there before and is not one of the review's findings.
