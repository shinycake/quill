# Linux OS idle time beyond GNOME

Quality sweep finding 27 (idle part). `os_idle_ms` on Linux asked only GNOME
Mutter and, on the first error, gave up for the whole run. On KDE, X11 window
managers and wlroots compositors auto-lock and presence never saw idle time.

## What tdesktop does

tdesktop reads `base::LastUserInputTime()` (lib_base, not in the local reference
clones, so its Linux backend list is unchecked here) for auto-lock and
notification presence. Quill's equivalent is `os_idle_ms`.

## What changed

`src/passcode/idle.rs` (moved out of `passcode/mod.rs`) models the source as
`IdleBackend`, probed once on the sampler thread in this order:

1. `Mutter`: `org.gnome.Mutter.IdleMonitor.GetIdletime` (ms, `u64`).
2. `ScreenSaver`: `org.freedesktop.ScreenSaver.GetSessionIdleTime` on
   `/org/freedesktop/ScreenSaver` or `/ScreenSaver`. KDE's implementation
   returns milliseconds as `u32` (it forwards `KIdleTime::idleTime()`).
3. `X11ScreenSaver`: MIT-SCREEN-SAVER `QueryInfo` through x11rb (new
   `screensaver` feature of the existing dependency), `ms_since_user_input`.
   Only when `SessionEnv` says the session is real X11: `XDG_SESSION_TYPE=x11`,
   or no usable session type with `WAYLAND_DISPLAY` unset and `DISPLAY` set.
   XWayland only sees X clients' input, so it is never used on Wayland.

`select_backend` and `candidate_backends` are pure and unit-tested. The first
backend that answers is kept; if it later stops answering the candidates are
probed again. `IdleCache::Failed` now means "no backend answered".

Unchanged: readers never block (they read an extrapolating cache), the
sampler pauses when nobody reads, and a restart clears the stale sample.

## Not done

- ext-idle-notify-v1 (wlroots, and Wayland generally): `wayland-client` is
  linked through GPUI, but `wayland-protocols` with the `staging` feature
  would be a new direct dependency plus our own Wayland event queue and
  seat binding on the sampler thread. Follow-up.
- Nothing here ran on real KDE/X11/GNOME hardware; the Linux build is only
  checked by Linux CI (no cross C toolchain on the dev Mac).

## How verified

Unit tests (`cargo test --features ui --lib passcode::`): backend selection,
environment detection, cache extrapolation and failure. The D-Bus and X calls
are Linux-only and need Linux CI or real desktops.
