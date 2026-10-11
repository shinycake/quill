# Tray and taskbar toggles, monochrome tray icon

Parity item: `parity:data-tray-toggles`.

## What tdesktop does

`settings/sections/settings_advanced.cpp` (`BuildSystemIntegrationSection`)
stores one `WorkMode` (window and tray, tray only, window only) and shows it
as two checkboxes:

- "Show tray icon" when `Platform::TrayIconSupported()` (macOS and Windows
  always, Linux when a StatusNotifier host exists).
- "Show taskbar icon" when `Platform::SkipTaskbarSupported()`: Windows
  always, Linux only on X11 with `_NET_WM_STATE_SKIP_TASKBAR`, never macOS.
- "Use monochrome icon" (`trayIconMonochrome`, default on) when
  `Platform::HasMonochromeSetting()`: Windows when the taskbar theme is
  readable from `SystemUsesLightTheme`; Linux when the icon theme has the
  `-symbolic` panel icon; never macOS. It is shown only while the tray icon
  is on.

The two work-mode switches are never both off: unticking one ticks the other.
On Windows the monochrome icon is a white or near-black glyph matching the
taskbar, with a small dot at the bottom left (red, gray when muted) instead of
the counter (`tray_win.cpp`, `MonochromeIconFor`, `MonochromeWithDot`).

## What was already in Quill

"Show tray icon" and "Show taskbar icon" (X11 and Windows skip-taskbar through
`ui/window_control.rs`) existed, but as ad-hoc checks inside the Appearance
dialog, without the "never both off" rule, and with no monochrome icon.

## What changed

- `src/tray/work_mode.rs`: `WorkMode`, the per-platform option list
  (`options`), and the switch transitions (`toggle_tray`, `toggle_taskbar`,
  `skip_taskbar`). Pure, unit tested.
- `src/tray/monochrome.rs`: the one-color icon renderer (from the existing
  tray template mask, so no new asset), the process-wide switch mirror, and
  `dark_taskbar()` (Windows registry via `RegGetValueW`; `None` elsewhere).
  The Windows tray uses it when the switch is on and the theme is readable.
- `src/ui/tray_settings.rs`: the three switches in tdesktop's order, and
  `apply_taskbar_icon` (moved out of `ui/appearance/mod.rs`). Turning the tray
  off now turns the taskbar icon on and vice versa, and both apply live.
- `AppearancePrefs.tray_icon_monochrome` (default true; old files load as on).

## Per-platform result

| | tray | taskbar | monochrome |
|---|---|---|---|
| macOS | yes | no | no (menu bar icon is already a template) |
| Windows | yes | yes | yes, if the taskbar theme is readable |
| Linux X11 | if a tray host exists | yes | no |
| Linux Wayland | if a tray host exists | no | no |

Linux has no monochrome switch because tdesktop only offers it when a
`-symbolic` icon is installed in the theme, and Quill ships none. No
vendored GPUI change was needed.

## How verified

- `cargo test`: work-mode transitions, option lists per platform, monochrome
  renderer (theme colors, dot), prefs default and round trip.
- `cargo check --target x86_64-pc-windows-msvc --no-default-features --lib`
  compiles the Windows registry code.
- Not verified: the Windows tray appearance and the X11 taskbar hint on real
  desktops, and a settings screenshot (the macOS build shows only the tray
  switch).
