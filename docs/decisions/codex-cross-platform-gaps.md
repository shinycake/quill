# Cross-platform gap audit

Rule: Linux and Windows work as well as macOS; a macOS-only feature needs
another backend or a clean, explained fallback. This audits every
`target_os = "macos"` gate in `src/` (41 files) and lists what changed.

tdesktop reference: it is a Qt app, so most of these features are one code
path per OS behind `base/platform/*` (`base_last_input_*`,
`base_system_settings_*`, `notifications_manager_*`); nothing is macOS-only
by design except the dock/menu-bar specifics.

## Table

| Feature | macOS | Linux | Windows | Action |
| --- | --- | --- | --- | --- |
| Notifications (`notify.rs`) | `UNUserNotificationCenter` via GPUI; click opens chat | `notify-send --wait` with Open / Reply / Mark as read actions (daemon dependent) | WinRT toast via GPUI, AUMID + icon registered; body click opens chat | Fine. Follow-up: toast Reply / Mark-as-read buttons (needs GPUI toast action API or the `windows` toast XML directly) |
| Voice input (`voice_input.rs`) | cpal; permission hint names System Settings | cpal; plain error | cpal; hint names Windows privacy settings | Fine (only message text differs) |
| System unlock (`ui/system_unlock.rs`) | LocalAuthentication (Touch ID / password) | none: switch hidden, passcode only | none: switch hidden, passcode only | Follow-up: Windows Hello (`UserConsentVerifier`, `windows` crate `Security_Credentials_UI`), Linux polkit (`org.freedesktop.PolicyKit1` over zbus). Larger than this PR |
| Message menu "Show in Finder" (`ui/message_menu_ui.rs`, `message_menu.rs`) | "Show in Finder" | "Show in folder" | "Show in folder" | Fine, label only |
| Keybindings (`ui/keybindings.rs`, `composer_shortcuts.rs`, `media_viewer.rs`) | `cmd-` | `ctrl-` | `ctrl-` | Fine, `primary!` macro; tests assert per OS |
| Cmd/Ctrl+Up reply guard (`shortcut_pack.rs`) | Skips when the draft is non-empty (Cmd+Up moves the caret) | Always replies | Always replies | Fine, matches tdesktop |
| Passcode OS idle (`passcode.rs`) | CoreGraphics | **was `None`**; now GNOME Mutter idle monitor via `gdbus`, `None` elsewhere | `GetLastInputInfo` | Done: Linux GNOME; other desktops keep the in-window fallback |
| Reduce-motion (`main.rs`) | `defaults read ...reduceMotion` | gsettings `enable-animations` | **was `false`**; now `SPI_GETCLIENTAREAANIMATION` | Done |
| Close / minimize to tray (`app_render.rs`, `main.rs`, `tray.rs`) | `cx.hide()` keeps app alive with tray | closing the window quits; tray via SNI still offered for "Start in tray" | same | Follow-up: GPUI Linux/Windows windows cannot be hidden after creation (see the note on `tray_setting_switches`), so the setting is hidden there today. Needs a GPUI patch |
| Tray icon (`tray.rs`, `tray_mac.rs`, `tray_sni.rs`) | NSStatusItem template icon | StatusNotifierItem | tray-icon | Fine |
| Dock / taskbar badge (`icon_badge.rs`) | dock tile | Unity launcher / desktop entry | taskbar overlay icon | Fine |
| Media keys / now playing (`media_session/`) | MPNowPlayingInfoCenter | MPRIS | SMTC | Fine |
| Autostart (`autostart.rs`) | LaunchAgent | XDG autostart `.desktop` | registry Run key | Fine |
| Default link handler (`link_handler.rs`) | NSWorkspace button | xdg-mime switch | registry switch | Fine |
| Spellcheck (`ui/spellcheck_*.rs`) | NSSpellChecker | Hunspell dictionaries | system spelling, Hunspell fallback | Fine |
| Copied files paste (`ui/clipboard_files.rs`) | NSPasteboard file URLs | text `file://` / path heuristics | GPUI `CF_HDROP` | Fine |
| Capture permission (`ui/capture_access.rs`) | AVFoundation prompt | none exists; capture error explains | OS privacy setting, error hint | Fine |
| Block screen capture in secret chats (`ui/capture_block.rs`) | `NSWindowSharingNone` | not possible; note shown | `SetWindowDisplayAffinity` | Fine, explained fallback |
| Mono font (`ui/message_text.rs`) | Menlo | monospace | Consolas | Fine |
| Inline / viewer video (`ui/native_video.rs`, `inline_video.rs`, `message_media.rs`) | AVFoundation native layer | `quillvideo` decode path in viewer; inline tile is an empty placeholder | same as Linux | Follow-up: inline video tiles off macOS (render decoded frames as GPUI images, as the viewer does) |
| Picture-in-picture (`ui/video_pip.rs`, `media_viewer.rs`) | floating NSWindow | status note "requires native floating-window support"; button hidden in viewer | same | Follow-up: always-on-top borderless GPUI window |
| Screen share in calls (`ui/call_panel.rs`, `calls/engine/ntgcalls.rs`) | ScreenCaptureKit via ntgcalls | button hidden | button hidden | Follow-up: ntgcalls desktop capturer on X11 / Wayland portal and DXGI |
| Link device with camera QR (`ui/device_qr.rs`, `ui/security.rs`) | `quill-qr-scanner` helper | button hidden (user can paste the link) | button hidden | Follow-up: camera frames through `nokhwa` + `rqrr` |
| Emoji rasterizing for the editor (`ui/editor_art.rs`) | AppKit text drawing | `None`: emoji stickers unavailable in the photo editor | same | Follow-up: rasterize with the font stack GPUI uses (`cosmic-text`) |
| Main window raise on tray open (`main.rs`) | NSWindow ordering | GPUI `activate_window` path | same | Fine |
| Heap trim (`ui/image_budget.rs`) | `malloc_zone_pressure_relief` | `malloc_trim(0)` on glibc | none needed, the heap decommits on its own | Fine |
| Word-jump keys in composer tests (`ui/composer_rtl.rs`) | `alt-left/right` | `ctrl-left/right` | same | Fine |
| Executable file warning list (`file_prefs.rs`) | mac extension set | Linux set | Windows set | Fine |
| ffmpeg install hints, library names, rlottie names | per OS | per OS | per OS | Fine |

## What changed

- `src/passcode.rs`: `os_idle_ms()` on Linux asks GNOME's
  `org.gnome.Mutter.IdleMonitor.GetIdletime` through `gdbus` (no new crate).
  Sampled by a lazily started background thread every two seconds (reads never block, `os_idle_known()` is render-safe), extrapolated in between; a failure
  (no `gdbus`, KDE, wlroots) is cached forever and callers fall back to
  in-window input. `parse_gdbus_idletime` is pure and unit-tested.
- `src/main.rs`: `os_prefers_reduced_motion()` on Windows reads
  `SPI_GETCLIENTAREAANIMATION` through `windows-sys` (already a dependency).

## Verification

- Gate (fmt, clippy, tests) on the host.
- `cargo check --target x86_64-pc-windows-msvc --features ui` cannot run here
  (the GPUI build script needs `llvm-rc`), so the Windows FFI was checked
  against the `windows-sys` 0.61 signatures by hand. The Linux idle code is
  gated to Linux; its parser test runs everywhere.
- Not verified on a live GNOME or Windows session.
