# Tray menu and startup

The existing native tray now offers Open Quill and Quit Quill. Its menu events use the existing UI timer, with exact menu-ID routing so unrelated menus cannot trigger them. On macOS, native close and Cmd-W hide the retained window when a tray is available; without a tray, ordinary close behavior remains. Initial sidebar focus fixes window actions on a newly opened, unfocused window.

General settings persist Start in tray through the existing appearance preferences; `--start-minimized` provides the same startup behavior. The initial window is hidden and the tray is initialized immediately. If no tray host exists, startup brings the window forward rather than leaving Quill inaccessible.

Real native Mac verification: `bash scripts/macos-tray-smoke.sh` and `bash scripts/macos-tray-smoke.sh target/debug/quill --start-minimized` both pass. They launch an owned process with an unavailable TDLib path, use System Events to open the native tray menu, exercise Cmd-W, restore the retained window, exercise the native close button, and quit. The minimized run also verifies no window was displayed before opening. The initial failed Cmd-W check exposed the startup focus bug; it is not represented as a pass. Core tests, strict core clippy, and local native UI build pass. Settings roundtrip includes the new preference.

The full minimize/close-to-tray checkbox remains unchecked: native minimization and Linux close-to-tray still need work. This slice declares only tray-menu and start-minimized, with native Mac evidence; Linux runtime verification remains pending.
