# Platform I/O off the UI thread

## What tdesktop does

tdesktop talks to the launcher entry, the StatusNotifier host and the idle
monitor through Qt's async D-Bus calls and signals, so a slow bus never blocks
the main loop. Pattern wallpapers are prepared on a background task and the
chat paints the plain fill until the image is ready.

## What changed in Quill

- **Launcher badge (Linux)**: `emit_launcher_entry` no longer runs `dbus-send`
  with `.status()` on the UI thread. The count goes into a latest-wins
  `LatestSlot`; a worker thread owns a zbus session connection and emits the
  Unity `LauncherEntry.Update` signal. A burst of changes while the worker is
  busy collapses to the newest count. A failed emit drops the connection and
  retries on the next change.
- **Tray startup (Linux)**: `poll_startup` and its 1.5 s `recv_timeout` are
  gone. `sync_tray_startup` only starts the registration (the existing worker
  thread). For start-in-tray, `main.rs` polls `tray_registering` /
  `tray_available` every 100 ms from an async task and reveals the window if
  no host shows up within `TRAY_STARTUP_WAIT_MS` (decision in the pure
  `tray_startup_reveal`).
- **Idle sampler (Linux)**: replaced the `gdbus` subprocess every 2 s with a
  zbus `GetIdletime` call. The sampler runs only while `os_idle_ms` is being
  read (the auto-lock tick reads it only when auto-lock is armed and
  unlocked); it pauses once reads stop for 5 s (`idle_sampling_wanted`) and
  restarts on the next read, dropping the stale sample first. First failure
  still stops it for good; the cached read API is unchanged.
- **Pattern wallpaper**: the pattern file is read and rasterised on a worker
  thread (`LoadBoard` tracks in-flight and finished loads). While pending the
  canvas paints nothing (the plain fill shows) and requests animation frames
  until the tile is ready.
- `src/system_accent.rs` is not on main yet; nothing to check there.

## How verified

Unit tests for the pure parts: `LatestSlot` coalescing and wake-up,
`idle_sampling_wanted`, `tray_startup_reveal`, `LoadBoard`. The gate passes on
macOS. The Linux-only paths (zbus emit, idle sampler, SNI registration) are
not compiled locally and are covered by CI on Linux; they were not run against
a real session bus.
