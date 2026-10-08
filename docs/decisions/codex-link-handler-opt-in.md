# Opt-in default handler for Telegram links

Branch: `codex/link-handler-opt-in`

## Decision

Quill never makes itself the default `tg:` handler on its own, on any
platform. PR #445 did so implicitly (Windows wrote the registry on startup,
Linux's installer ran `xdg-mime default`). Both are removed. The user turns
it on in Settings (Appearance dialog, next to "Launch at login"):
"Open Telegram links with Quill". In-app link handling is unchanged.

tdesktop has no equivalent switch (it registers on every start and on
install); the opt-in is a Quill decision so people who also run Telegram
Desktop keep their links.

## Per platform

| OS | Control | Turn on | Read state | Turn off |
| --- | --- | --- | --- | --- |
| macOS | "Make Quill the default" button; status "Quill is the default" | `NSWorkspace setDefaultApplicationAtURL:toOpenURLsWithScheme:` (macOS 12+, the system asks the user to confirm) | `URLForApplicationToOpenURL(tg://resolve)` path compared with our `.app` | not possible |
| Linux | switch | `xdg-mime default quill.desktop x-scheme-handler/tg` | `xdg-mime query default x-scheme-handler/tg` | nothing is restored (note under the switch) |
| Windows | switch | writes `HKCU\Software\Classes\tg` (URL Protocol, `DefaultIcon`, `shell\open\command` = `"exe" -- "%1"`) | `open` command: a `quill.exe` command means on | deletes `HKCU\Software\Classes\tg`, only when the command is a Quill's |

### Why a button on macOS

macOS has no API to give the scheme back to another app (the replacement is
chosen from that app). A switch that cannot switch off would lie, so the
macOS control is a one-way button; once Quill is the default it is replaced by
"Quill is the default" and the note says to use the other app to change it.
The status always reflects reality (re-read after the confirmation prompt at
1 s, 3 s and 8 s, and cached for 2 s otherwise). An unbundled binary
(`cargo run`) has no bundle to register, so the button is disabled with a
reason.

### Windows ownership

A registration counts as Quill's when the command's executable is named
`quill.exe` (any folder, so a previously self-registered or moved install
shows as on and can be removed or refreshed). Another client's registration
(Telegram Desktop) is never removed; turning the switch on explicitly
overwrites it because the user asked.

## Unchanged

macOS `CFBundleURLTypes` in the Info.plist and `MimeType=x-scheme-handler/tg;`
in `quill.desktop` stay: they only make Quill selectable, not the default.
`src/scheme_registration.rs` is replaced by `src/link_handler.rs` (the entry
table moved there; the startup call in `main.rs` is gone).

## Tests

Pure unit tests: xdg output parsing and command args, the Windows entry
table and ownership rule, macOS bundle detection and state. Windows registry
access reuses `winreg` (now with a non-test `delete_tree`) and compiles in
the `windows-build` CI job.

## Check live

- macOS: Settings shows the current handler; the button opens the system
  confirmation. Do not run it if you rely on Telegram Desktop.
- Linux: `xdg-mime query default x-scheme-handler/tg` before/after the switch.
- Windows: `reg query HKCU\Software\Classes\tg\shell\open\command`.
