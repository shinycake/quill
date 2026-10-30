# Idle redraws: what still drew with nothing changing on screen

Each frame costs CPU, and on macOS a window that keeps drawing never lets its display link go idle, so the
idle release of its spare drawable (`idle-drawables.md`, about 20 MB at a usual window size) never runs. This
pass looked for what still draws in an idle window, active or not, after `codex-idle-redraw.md`,
`codex-idle-cpu.md`, `sweep-idle-app-timers.md`, `sweep-idle-heartbeat.md` and #1244 (the caret blinks in an
active window only).

## How it was looked for

`frame_clock::frames_over` (tests only) advances the test clock in 100 ms steps and counts what the window
drew: one `(window frame)` per frame (a test window draws as soon as it is dirty), each cached slice that
rendered, and `(next frame)` for each animation-frame callback. A sweep over 66 demo fixtures, plus a paused
track, a focused composer and an open Settings dialog, measured 10 s in an active window and 10 s in an
inactive one after a settle; the cases below came out of it. Timers, `spawn` loops,
`request_animation_frame` and `on_next_frame` sites were also read.

## Fixed

### A paused track redrew four times a second

`audio_playback.rs` `spawn_playback_tick`: the 250 ms progress tick runs while a track is active, playing or
paused (it also drains the media keys), and each tick redrew the open chat's conversation and composer slices
and every chat window (`notify_playback_position`). A paused voice note or song left in the player bar drew 4
frames a second, active or not, for as long as it stayed paused. The tick now redraws only when what the seek
bars and the player bar show of the sound changed (`PlaybackUi::sound_shown`: engine state, position, error).
Playing, the position moves every tick and it redraws as before.

### An unplayed message effect drew at the display's rate behind another app

`message_effect_badge.rs` `badge`: the badge of a message sent with an effect that has not played asks for
another frame on every paint, so that the next frame's pass marks it seen and starts the effect. Nothing
counts as seen while the window is inactive (`shown_unwatched`, tdesktop `markingContentsRead`), so a
message with an effect that arrived in the open chat behind another app redrew the window at the display's
rate (120 a second on ProMotion) until the window was activated. The badge now asks only in an active window;
activation redraws everything, and the effect starts then as before.

### The caret's blink under the code editor's menus (gpui-base)

Leftover from #1244. The input's blur handler returns early while its completion or code-action menu is open,
so the menu keeps the field looking focused. A window deactivated with that menu open left the blink running
(the inactive window repainted twice a second), and so did a menu that closed on a field that had lost its
focus under it. Deactivating the window now stops the blink, and a blink that finds nothing to show (the field
is neither focused nor under its menu, or the window is inactive) stops instead of repainting
(`third_party/gpui-base/QUILL-CHANGES.md`). Only the kit's code editor (`EditorState`) has these menus; Quill's
fields are `Input`/`Textarea`, whose blur handler never returns early, and the composer's right-click menu is
the native one on macOS (focus stays) and a drawn one elsewhere (the field blurs and stops the blink as
usual). So no Quill window drew from this; the fix keeps it from coming back with an editor.

## Tests

- `idle_frames_tests` (`src/ui/frame_clock.rs`): an open chat with the composer focused draws nothing over a
  simulated minute behind another app; in front, only the caret draws (the composer slice, about twice a
  second); a Settings dialog draws nothing behind another app; a paused voice note draws nothing in front
  or behind another app, and playing again moves the seek bar.
- `send_effects::tests::an_effect_that_arrives_behind_another_app_waits_without_drawing`: no frame in a
  minute, the effect still unplayed, and it plays on activation.
- gpui-base `an_open_completion_menu_does_not_blink_the_caret_in_the_background` and
  `a_caret_blinking_under_its_menu_stops_when_the_menu_closes_without_focus`.

On the old code each of these fails: the paused voice note drew 240 frames in the simulated minute in front
(4 a second, each redrawing the conversation and the composer), the effect 600 behind another app (one per
100 ms test step; on a Mac, one per display refresh), and the caret tests counted 6 repaints in 3 s where 0
are expected.

## Found, not changed

- In front with the composer focused, the caret blinks twice a second for as long as the window stays in
  front, which keeps a second drawable. That is a visible change (macOS, Windows and tdesktop blink without a
  limit; GTK stops after 10 s idle, `gtk-cursor-blink-timeout`), so it is left to a product decision.
- The kit's `Skeleton` and `Spinner` animate at the display's rate through GPUI's `with_animation`, in an
  inactive window too: GIF panel tiles whose thumbnail is not there yet (`gifs_ui.rs` `Skeleton`), the topic
  history loading spinner, the GIF search spinner. They are on screen and moving, so they are not idle; a
  lower rate (as the typing dots run at 12 fps) or a pause behind another app would be a separate change.
- A recording voice message and a playing track redraw as they should (the timer and the seek bar move).

## Not a loop: the kit scrollbar's fade under the test clock

Fixtures whose history moves the scroll offset while it loads (albums, media, code blocks, inline video,
the bot keyboard, among others) showed a few conversation redraws a second in the sweep, active and
inactive. The kit scrollbar (`gpui-base` `scrollbar.rs`, `Scrollbar::prepaint`) shows itself when the offset
changes and hides 2 s later: it reads the wall clock (`Instant::now()`) but waits on the executor's timer.
Under the test clock the timer's 2 s pass in a few real milliseconds, the wall clock says the 2 s are not
over, and the timer is armed again, until 2 s of real time have passed. With the test thread sleeping 2.5 s
of real time first, eight of these fixtures drew the one fade and then nothing over the next 10 s in front
(but the caret) and 20 s behind another app. On a real
clock it is one fade 2 s after the offset last moved. An idle test that counts frames should open a fixture
that does not move the offset after it opens (`ready-chats` does not), or the count depends on how fast the
machine runs the test.
