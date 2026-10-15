# Display change events (GPUI 0.3.9)

GPUI 0.3.9 (with GPUI Kit 0.7.3) reports display changes instead of leaving
apps to poll:

- `App::observe_displays(|DisplayEvent, &mut App|)`: a display was added,
  removed, or its bounds or refresh interval changed.
- `Window::observe_window_display` / `Context::observe_window_display`: the
  window moved to another display, or its display changed.
- `Window::refresh_interval(&App)`: the window's display's frame interval,
  cached per display by GPUI (no OS query per call). Variable refresh
  displays report their maximum rate.

Platform support, read from the vendored sources: macOS (`windowDidChangeScreen`,
the screen-parameters notification), Windows (`WM_DISPLAYCHANGE`, the window's
center crossing to another monitor on move, the rate from
`EnumDisplaySettingsW`) and Wayland (output enter, `wl_output` modes) report
all three. X11 reports no display events and no refresh interval: its
"display" is the X screen, which spans every monitor and never changes for a
window.

This PR uses them where they make Quill more correct or cheaper. The rest of
this page is what was looked at and why it stayed as it is.

## What changed

### 1. The main window's display is noted on change, not every frame

`flush_notifications` (run from every `QuillApp` render) did
`window.display(cx)` to remember where notification windows go by default
(tdesktop `NotificationDisplayRect(activePrimaryWindow)`). `Window::display`
enumerates every display through the platform (`CGGetActiveDisplayList` on
macOS; `EnumDisplayMonitors` plus `GetMonitorInfoW`, `GetDpiForMonitor` and
a UUID per monitor on Windows) and searches it, once per frame, for a value
that changes only when the window moves to another monitor.

`ui/display_events.rs` now notes it once when the app is built and again from
`observe_window_display`. On X11 the window's display never changes, so the
first value stays right.

### 2. Display added, removed or resized: notification windows follow

`observe_displays` → `custom_notify_displays_changed`:

- Notifications that arrived while there was no display stay queued and are
  shown when one is connected. Before, they were dropped ("a queue nothing
  drains"); tdesktop keeps them and shows them on `QGuiApplication::screenAdded`
  (`Window::Notifications::Default::Manager`). A newer notification of the
  same chat still replaces a queued one, so the queue stays one per chat.
- Shown windows are put back in place on the current notification display
  (the next clock step moves them). Before, they were only moved while an
  animation was running, so a monitor unplugged or a resolution change left
  them wherever the OS put them until the next notification.
- The settings' "Display for notifications" list redraws, so plugging in a
  monitor with the settings open shows it.

A change of refresh rate alone does neither (`display_events::reaction`).

### 3. Video pacing uses the display's reported refresh period

`video_player::pacing::PresentEstimator` predicts when a frame reaches the
screen and how long a player may sleep between pictures. On macOS it had the
display link's period. Elsewhere it guessed 60 Hz until it had seen two draws,
then learned the interval from draw spacing, which a player that sleeps
between pictures blurs (it draws at multiples of the interval). The element
now passes `Window::refresh_interval` as the period where the link has none:
Windows and Wayland get the real rate (120, 144, 165 Hz…) from the first
frame, and X11 keeps learning it. A 60 fps clip starting on a 144 Hz display
now sleeps between pictures instead of asking for every frame while the
estimate still says 60 Hz (test
`a_reported_period_paces_the_first_frames_instead_of_60hz`). The estimator
already ignores periods outside 20 to 240 Hz.

## Considered and left alone

### Inline video scale check (`InlineVideos::frame_start`)

It compares the window's scale with last frame's and restarts the players
(which decode at the old size) when it changed. That is one float comparison
per frame, so an event cannot make it cheaper, and it must stay: the scale
also changes with Quill's interface zoom, which is no display event. A move to
a display with the same scale needs no restart, and the comparison already
does not restart then. tdesktop does the same: streaming frames are requested
at the paint device's current `devicePixelRatio` on every paint.

### Animation clocks (frame clock, notification windows)

Nothing paces animations by an assumed 60 Hz display:

- `frame_clock` ticks at the rate the content asks for: stickers up to 60 fps
  (Lottie's own cap, lib_lottie `kMaxFrameRate = 60`), custom emoji and inline
  video at 30. The 60 clamp is the content's rate, not the display's.
- lib_ui runs every `Ui::Animations` animation from one timer at
  `kAnimationTick = 1000 / st::universalDuration` = 8 ms (120 Hz, fixed), and
  Lottie players from timers at their frame times. tdesktop never reads the
  display's refresh rate (no `QScreen::refreshRate` anywhere in tdesktop,
  lib_ui or lib_lottie).
- Notification windows slide on a 16 ms timer (`notify_windows::FRAME`).
  Following the display rate would mean up to 144 or 240 OS window moves a
  second; tdesktop's 8 ms tick is already above Quill's here, so this was not
  raised.

Snapping the frame clock's period to whole display intervals was considered
(even cadence on 144 Hz) and rejected: rounding down raises the rate and the
CPU, rounding up drops a 60 fps sticker to 48 fps on 144 Hz and to 37.5 fps on
75 Hz, and a timer that is not phase-locked to vsync still jitters either way.

### Picture-in-Picture placement

It opens at a fixed origin and size with a minimum size; nothing per display
is cached, so there is nothing to refresh. tdesktop's `PipPanel` adjusts its
maximum size on `screenChanged`; GPUI has no maximum window size, so there is
nothing to update yet.

### Window restore (`main.rs`), tray and menu bar

Restoring the saved window bounds reads the displays once at startup. The
macOS tray and menus do not depend on displays.

### The macOS display lookup in `video_frame`

`present_hint` still calls `window.display(cx)` per video draw to find the
display link of the window's display: GPUI exposes the window's display only
through that call. Caching it via `observe_window_display` would be the next
step if it ever shows in a profile.

## Measurements

Display enumerations from `flush_notifications`: one per `QuillApp` render
before (5 a second in the `ready-sticker-playback` fixture by
`QUILL_TRACE_TICKS`, and every frame while scrolling or typing), none after.
The cost of one enumeration depends on the platform and could not be measured
in the Linux cloud container (on X11 it only reads the connection setup).

Idle CPU, a Linux take on `scripts/idle-cpu-bench.sh` (debug build with
`demo-capture`, Xvfb and the lavapipe software Vulkan driver, mean %CPU from
`/proc/<pid>/stat` over 30 s after a 15 s settle, `QUILL_ASSUME_ACTIVE=1`,
builds alternated):

| Fixture | Before (run 1 / 2) | After (run 1 / 2) |
| --- | --- | --- |
| `ready-chats` (idle) | 0.03% / 0.07% | 0.03% / 0.03% |
| `ready-sticker-playback` | 138.6% / 107.6% | 111.5% / 99.0% |
| `ready-gif-playback` | 118.4% / 96.8% | 96.3% / 85.6% |

No regression. The container's 4 cores were shared with other jobs (load
average about 8), so the animated fixtures vary by 20 points between runs;
the lower "after" numbers are within that noise and are not claimed as a
gain. On X11 neither change should move CPU: X11 reports no refresh interval
(video pacing is unchanged there) and its display enumeration is cheap. The
pacing change matters on Windows and Wayland, which this container cannot
run.

## Verification

- Tests: `display_events::reaction` (added, removed, bounds and refresh-only
  changes); a GPUI test on the test platform that the app notes its window's
  display when built and redraws when a display is connected
  (`simulate_display_added`); `pacing` test that a reported 144 Hz period
  paces a clip's first frames and lets a 60 fps clip sleep between pictures.
- Full Linux gate. Headless run of `ready-gif-playback` under Xvfb: the clip
  plays.

Not verified here: the macOS `present_hint` change (the display link's period
still wins; GPUI's rate is only the fallback) is not compiled by Linux CI, and
nothing was run on Windows, Wayland or macOS. Moving the window between
monitors, plugging and unplugging a monitor with notification windows shown,
and the settings' display list need a check on a multi-monitor Mac or Windows
machine.
