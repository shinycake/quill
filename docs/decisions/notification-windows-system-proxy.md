# Notification windows and the system proxy

Covers `parity:notify-desktop-options` and `parity:data-proxy-extras`. It
replaces the "not built" parts of `codex-notify-desktop-options.md` and the
"Skipped" section of `codex-proxy-extras.md`.

## Notification windows

### What tdesktop does

`window/notifications_manager_default.cpp` (`Default::Manager`) draws its own
always-on-top windows when the system's notifications fall short
(`Platform::Notifications::{Supported, Enforced, ByDefault}`):

- macOS: native only (enforced), unless the "Force non-native" option is on.
- Windows: its own windows by default, with a "Use Windows notifications"
  switch.
- Linux: native when the D-Bus daemon has body, actions, inline reply and
  sound or inhibitions; otherwise its own windows. Wayland is enforced
  native (no window positioning).

The windows are 320 wide, 80 tall, stacked from the chosen corner with the
newest nearest the corner, at most "Notifications count" (1-5, default 3)
with the rest queued, plus a "Hide all" button while more than one is up or
the queue is not empty. They fade in, slide when the stack changes, wait
for user input, then fade slowly 3 s later. Hovering one keeps all of them
up. Click opens the chat, right click or the close button closes, Reply
opens a field in the window.

`settings_notifications.cpp` shows the native switch, "Respect system Focus
mode" (Windows), "Display for notifications" (two or more displays),
"Location on the screen" (hovering a corner shows sample windows there and
dims the real ones) and "Notifications count". The last three only while the
custom windows are used.

### What Quill does

- `quill::notify::custom`: the backend choice, layout, stacking, queue and
  helpers, with unit tests.
- `ui::notify_windows`: the windows (GPUI `PopUp`, transparent background,
  content opacity for the fade since GPUI has no window opacity), a clock
  task that ticks every frame only while something animates, and native
  window moves (`position.rs`: `setFrameTopLeftPoint` on macOS,
  `SetWindowPos` on Windows, X11 `ConfigureWindow`), since GPUI cannot move
  a window.
- `view.rs` uses kit `Button`, `Textarea` and the chat avatar; a locked app
  or hidden sender shows the Quill logo and name.
- Clicks go through the same queue the system notifications use, so opening
  a chat, switching accounts and inline replies behave the same.
- Settings: `ui::notification_settings::desktop_options`, stored in
  `notify_prefs.json` (`native`, `corner`, `count`, `display`,
  `skip_in_focus`).
- "Force non-native" is the environment variable
  `QUILL_CUSTOM_NOTIFICATIONS=1`, as tdesktop keeps it in its experimental
  options.

### Platform notes

- X11: GPUI reports one display covering the whole X screen with no work
  area, so the display choice is hidden, positions are relative to that
  screen and a corner window can sit over a panel. Without a compositor the
  transparent fade may show black. A pop-up window there never gets the
  keyboard, so Reply opens the chat in the main window instead of a field.
- Windows are closed on passcode lock, account switch and sign out: they
  belong to the account on screen.
- A Linux system without a notification daemon is asked again every 30 s.
- Wayland and macOS (unforced) keep the system notifications, as tdesktop.
- The display is identified by its UUID when the platform gives one, else
  by its id.

## Use system proxy settings

tdesktop takes the system proxy from Qt (`QNetworkProxyFactory`) and makes it
the default mode. TDLib only takes explicit proxies, so Quill reads the OS
setting itself (`quill::system_proxy`) and keeps it as a hidden TDLib proxy
tagged "Quill: system proxy":

- macOS: `scutil --proxy` (SOCKS, then HTTPS, then HTTP).
- Windows: `HKCU\...\Internet Settings` `ProxyEnable` and `ProxyServer`.
- Linux: `all_proxy`, `https_proxy`, `http_proxy`, then GNOME
  `org.gnome.system.proxy` in manual mode.
- A background reader re-reads every 15 s, only while the mode is "Use
  system proxy settings" and no custom proxy is on; the connection driver
  adds, edits, enables or removes the hidden entry one step at a time. A
  step TDLib refuses waits 5 minutes (or a new OS setting) and its error
  stays out of the proxy box.
- PAC scripts are not evaluated.
- The entry is not in the list, has no QR or share, is not used for calls and
  is not in auto-switch, like tdesktop.
- The proxy box has the three radios. Deleting the enabled proxy falls back
  to "Use system proxy settings", and clicking the enabled row does nothing,
  as in tdesktop. Existing installs start on "Use system proxy settings",
  tdesktop's default.

## Verified

Unit tests (backend choice, stacking, queue, proxy parsers and planner),
connect-driver tests for the system proxy, GPUI UI tests for the window
stack, queue, click and Hide all. Windows and X11 window moves are built in CI
but not run here.
