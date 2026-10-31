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

What else the paused tick happened to redraw had to notify for itself. One writer did not: a volume change
from the Picture-in-Picture window (`media_viewer/pip.rs` `pip_volume`) left the player bar's speaker icon
as it was; it now notifies, as the bar's own volume control does. The other writers of `PlaybackUi` notify,
or go through a call that does.

### An unplayed message effect drew at the display's rate behind another app

`message_effect_badge.rs` `badge`: the badge of a message sent with an effect that has not played asks for
another frame on every paint, so that the next frame's pass marks it seen and starts the effect. Nothing
counts as seen while the window is inactive (`shown_unwatched`, tdesktop `markingContentsRead`), so a
message with an effect that arrived in the open chat behind another app redrew the window at the display's
rate (120 a second on ProMotion) until the window was activated. The badge now asks only in an active window;
activation redraws everything, and the effect starts then as before.

### The caret's blink, by the window that draws the field (gpui-base)

Leftover from #1244. The input's blur handler returns early while its completion or code-action menu is open,
so the menu keeps the field looking focused. A window deactivated with that menu open left the blink running
(the inactive window repainted twice a second), and so did a menu that closed on a field that had lost its
focus under it. Only the kit's code editor (`EditorState`) has these menus; Quill's fields are
`Input`/`Textarea`, whose blur handler never returns early, so no Quill window drew from this.

The input's focus and activation listeners watch the window the field was made in, and a field can be drawn
in another one: the call window's message field (`group_call.composer`) is made by the main window, and a
main-window field can show in a dialog hosted by a chat window. Stopping the blink from those listeners (the
first version of this fix) left such a field's caret hidden once its window came back to the front, until a
click. The input's paint now keeps the blink running exactly while the caret can show in the window painting
the field (focused there or under its menu, and the window active): it stops a blink that shows nothing and
starts one no listener started. A window's activation change redraws it whole, so the paint runs then. Only
the window that started a blink stops it, so a field painted in two windows can't have them take turns. This
also stops a call-window field's blink when it loses its focus there, which no listener saw before
(`third_party/gpui-base/QUILL-CHANGES.md`).

## Tests

- `idle_frames_tests` (`src/ui/frame_clock.rs`): an open chat with the composer focused draws nothing over a
  simulated minute behind another app; in front, only the caret draws (the composer slice, about twice a
  second); a Settings dialog draws nothing behind another app; a paused voice note draws nothing in front
  or behind another app and stays where it was paused, and playing again moves the seek bar on every tick.
- `send_effects::tests::an_effect_that_arrives_behind_another_app_waits_without_drawing`: no frame in a
  minute, the effect still unplayed, and it plays on activation.
- `media_viewer::pip::tests::muting_the_pip_window_shows_on_a_paused_player_bar`.
- gpui-base `an_open_completion_menu_does_not_blink_the_caret_in_the_background`,
  `a_caret_blinking_under_its_menu_stops_when_the_menu_closes_without_focus`,
  `a_field_drawn_in_another_window_blinks_by_that_window` and
  `a_scrolled_bar_hides_on_the_app_clock_and_then_draws_nothing`.

On the old code each of these fails: the paused voice note drew 240 frames in the simulated minute in front
(4 a second, each redrawing the conversation and the composer), the effect 600 behind another app (one per
100 ms test step; on a Mac, one per display refresh), and the caret tests counted 6 repaints in 3 s where 0
are expected.

No test here depends on real time. The demos' simulated sound kept time with the wall clock
(`PlaybackClock`), so a paused-track test only worked if it paused the 12 s note within 7.2 s of real time;
the simulation now keeps the app's clock (`AudioEngine::set_sim_time`, the executor's, which is the wall
clock outside tests).

## Found, not changed

- In front with the composer focused, the caret blinks twice a second for as long as the window stays in
  front, which keeps a second drawable. That is a visible change (macOS, Windows and tdesktop blink without a
  limit; GTK stops after 10 s idle, `gtk-cursor-blink-timeout`), so it is left to a product decision.
- The kit's `Skeleton` and `Spinner` animate at the display's rate through GPUI's `with_animation`, in an
  inactive window too: GIF panel tiles whose thumbnail is not there yet (`gifs_ui.rs` `Skeleton`), the topic
  history loading spinner, the GIF search spinner. They are on screen and moving, so they are not idle; a
  lower rate (as the typing dots run at 12 fps) or a pause behind another app would be a separate change.
- A recording voice message and a playing track redraw as they should (the timer and the seek bar move).

## Not a loop, but tests counted it: the kit scrollbar's fade

Fixtures whose history moves the scroll offset while it loads (albums, media, code blocks, inline video,
the bot keyboard, among others) showed a few conversation redraws a second in the sweep, active and
inactive. The kit scrollbar (`gpui-base` `scrollbar.rs`, `Scrollbar::prepaint`) shows itself when the offset
changes and hides 2 s later: it read the wall clock (`Instant::now()`) but waited on the executor's timer.
Under the test clock the timer's 2 s pass in a few real milliseconds, the wall clock said the 2 s were not
over, and the timer was armed again, until 2 s of real time had passed. With the test thread sleeping 2.5 s
of real time first, eight of these fixtures drew the one fade and then nothing over the next 10 s in front
(but the caret) and 20 s behind another app. On a real clock it is one fade 2 s after the offset last moved,
so nothing changes for a user; but how many frames a test counted depended on how fast the machine ran it.
The scrollbar now reads the executor's clock, the wall clock outside tests, so the fade ends 2 s of test
clock after the scroll, and the idle tests settle 5 s before they count.
