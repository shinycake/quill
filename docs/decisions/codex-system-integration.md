# Cross-platform system integration (notifications, badges, autostart, tray)

Branch: `codex/system-integration`. Closes the audit gaps for Windows and Linux
and the missing macOS dock badge. Telegram Desktop references are read-only
(`Telegram/SourceFiles/platform/{mac,linux,win}/`).

## What the audit got wrong (verified in code first)

- **Notifications on Windows/macOS did not need a new WinRT layer.** GPUI 0.3.7
  already ships `show_system_notification` for macOS (`UNUserNotificationCenter`),
  Windows (WinRT `ToastNotificationManager`, AUMID registered under
  `HKCU\Software\Classes\AppUserModelId`) and Linux (`notify-rust`), plus
  `on_system_notification_response` for clicks. Quill only used it on macOS, and
  its response callback was already wired there (a body click already opened the
  chat; `actions` is empty on purpose because the body click is the action). So
  no `windows` WinRT code and no new notification crate: `winrt-notification`
  and `tauri-winrt-notification` would duplicate what GPUI already does, and
  the latter pulls a second `windows` major.
- **The Linux tray was broken, not just untested.** `tray-icon` on Linux creates
  the icon through libappindicator and `muda` GTK menus; its README requires
  `gtk::init` and a running GTK main loop on the creating thread
  (`~/.cargo/registry/.../tray-icon-0.21.3/README.md`). GPUI runs its own
  Wayland/X11 loop and never initializes GTK, so `Menu::new()` panics
  ("GTK has not been initialized") or the icon never appears. It also forced
  GTK dev packages into the Linux build graph.

## Changes

| Area | Before | After |
|---|---|---|
| Windows notifications | none (`build_notification_command` returned `None`) | GPUI WinRT toast via `NotifyBackend::Native`; click focuses the window and opens the chat; per-chat tag replaces the previous toast; `IconUri` registered so the toast shows the Quill icon; Quill plays its own sound through the in-process audio engine (#443), same path as macOS/Linux, because GPUI posts toasts silent |
| macOS notification click | callback existed, but a hidden/minimized window might never render to consume it | the response handler now activates the app and window itself; tag build/parse extracted to `notify::{notification_tag, parse_notification_tag}` |
| Linux notifications | `notify-send --wait --action=default=Open` | unchanged (click path kept) |
| macOS dock badge | none | `NSApplication.dockTile.badgeLabel`, "99+" cap, cleared at 0 |
| Windows taskbar badge | none | `ITaskbarList3::SetOverlayIcon` with the rendered count pill (`tray::render_overlay_icon`), retried until the taskbar button exists |
| Linux launcher badge | Unity LauncherEntry via `dbus-send` | unchanged |
| Windows autostart | unsupported | `HKCU\...\CurrentVersion\Run` value `Quill` = quoted exe path |
| Linux tray | `tray-icon` (needs GTK loop) | `ksni` StatusNotifierItem over D-Bus (pure Rust, zbus/async-io already in the tree), registration on a worker thread, retried every 10 s, `tray_available()` true only when registered |
| Tray switches | "Start in tray" always shown; "Minimize to tray" macOS-only | both hidden unless `tray_available()` (`tray::tray_setting_switches`) |

Unread numbers everywhere come from `tray::badge_count` with `BadgePrefs`; there
is no second definition.

## Decisions

- **Registry via `windows-sys`** (`src/winreg.rs`, `Win32_System_Registry`
  feature added): REG_SZ only, HKCU only, no elevation. Shared by autostart and
  toast identity.
- **`windows` crate features** `Win32_UI_Shell` and `Win32_UI_WindowsAndMessaging`
  added for `ITaskbarList3` and `CreateIcon`; the crate was already a dependency.
- **Launch-minimized.** tdesktop writes `-autostart` and hides the window only
  when its "start minimized" setting is on. Quill's equivalent is the persisted
  General "Start in tray" setting, read on every launch, so the Run value carries
  no flag (same for the XDG/LaunchAgents entries).
- **`tray-icon` moved to `cfg(not(linux))`, `ksni` added for Linux** (features
  `async-io` + `blocking`, matching GPUI's ashpd/zbus stack; no tokio). Flatpak
  sandboxes use `disable_dbus_name`. Startup waits up to 1.5 s for the first
  registration so `--start-minimized` can distinguish "no tray host" (reveal the
  window, existing behavior) from "tray still registering".
- **Dock badge needs notification authorization.** The Dock draws a badge only for apps the user allowed to badge (Settings > Notifications > Quill > Badge application icon). GPUI requests alert+sound only, so `icon_badge` requests badge+alert+sound itself before the first push (bundle-only; `QUILL_TRACE_STATUS=1` logs `badgeSetting`). macOS prompts only while the decision is open: if Quill was already decided without badge, or badges are off, the user must enable them in System Settings; there is nothing more the app can do.
- **Not done.** Windows toast reply action (tdesktop has inline reply): GPUI's
  toast API carries buttons but no text input; follow-up. Close-to-tray on
  Windows/Linux: GPUI windows cannot be hidden after creation there, so only
  start-in-tray exists; the macOS minimize-to-tray path is unchanged. 
- No README parity fragment: `platform-app-icon-badge`, `platform-autostart`,
  `platform-os-notifications`, `platform-tray-icon` are already checked; the
  merge pipeline owns the README prose.

## tdesktop references

- Windows toasts and AUMID: `platform/win/notifications_manager_win.cpp`
  (`ToastNotificationManager`, tag/group per peer).
- Taskbar overlay: `platform/win/main_window_win.cpp` (`SetOverlayIcon`).
- Run key: `platform/win/specific_win.cpp` (`Software\Microsoft\Windows\CurrentVersion\Run`).
- Dock badge: `platform/mac/main_window_mac.mm` (`NSDockTile`).
- Linux tray and launcher entry: `platform/linux/tray_linux.cpp`,
  `main_window_linux.cpp` (StatusNotifierItem, Unity LauncherEntry).

## Verification matrix

| Piece | Unit tests | Compiles | Run live |
|---|---|---|---|
| tag build/parse, backend selection, PowerShell quoting, AUMID key | yes (all hosts) | macOS + `cargo check --target x86_64-pc-windows-msvc` | not run |
| dock label, overlay description/icon render, tray tooltip, ARGB conversion, switch gating | yes (all hosts) | same | not run |
| macOS dock tile, toast click | n/a | macOS `--features ui` | to test by hand |
| Windows registry (`winreg`, autostart roundtrip) | yes, `cfg(windows)` tests in `windows-build` CI | Windows CI | CI only |
| Windows overlay, toast | n/a | Windows CI (`--features ui`) | needs a Windows machine |
| Linux ksni tray | n/a | `ksni-check` scratch crate on macOS; Linux CI (`linux-package`) | needs a Linux desktop |

Windows `--features ui` cannot be checked on this Mac (gpui's build script needs
`llvm-rc`), so the `windows-build` CI job is the compile check.
