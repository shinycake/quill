# Platform small fixes (Windows manifest, Linux desktop id, file names)

Source: quality-sweep platform audit, findings P2, P5, P7, P10. tdesktop reference: it ships a
Windows manifest with per-monitor DPI and `longPathAware`, and its Linux desktop file is named after
the app id (`org.telegram.desktop.desktop`) with a matching `StartupWMClass`.

## What changed

- **P2 manifest.** gpui-fast is built with `windows-manifest-provided-by-host`, and the repo had no
  RT_MANIFEST (build.rs only embedded the icon), so the host exe had to carry it. Added
  `assets/windows/quill.manifest` (PerMonitorV2 + dpiAware, longPathAware, UTF-8 active code page,
  Common-Controls 6, Win10/11 supportedOS, asInvoker) and build.rs now writes `1 24 "<manifest>"`
  next to the icon in the same .rc, so there is a single resource script (no duplicate-resource
  link hazard). `tests/windows_manifest.rs` checks the manifest settings and that build.rs embeds it,
  on every platform.
- **P5 desktop id.** `assets/quill.desktop` is now `assets/org.shinycake.quill.desktop` with
  `StartupWMClass=org.shinycake.quill`, matching the window app_id. Package script, installer
  (`linux-install.sh`, keeps a hidden `NoDisplay` `quill.desktop` alias only where an old one existed,
  so a previously chosen `tg://` default keeps working), CI smoke grep, the runtime-written
  per-user entry (`link_handler.rs`, `parse_xdg_default` still treats the old name as ours), the
  Launcher badge URI and the notification `desktop-entry` hint all use the new id. Test asserts the
  shipped file's StartupWMClass equals the app id.
- **P7 sanitizer.** `export::names::sanitize`: Windows device names (CON, PRN, AUX, NUL, COM1-9,
  LPT1-9, with or without extension) get a `_` prefix; trailing dots/spaces trimmed; truncation keeps
  the extension and caps at 120 chars and 200 bytes (leaves room for `_thumb.jpg`/` (n)` under the
  255-byte limit), on a char boundary.
- **P10.** Linux autostart honours an absolute `XDG_CONFIG_HOME`; `~` expansion
  (`path_expand::expand_tilde`) uses the platform home (USERPROFILE on Windows) and only expands `~`
  or `~/`; media Save de-duplication opens with `create_new` so it never overwrites.
  (Brief listed `src/media_viewer/save.rs` as owned despite the general media_viewer freeze.)

## Skipped

- **AltGr chord (`ctrl-alt-1`).** Not real: gpui's `input_preference` (window.rs) already prefers
  character input over key bindings for AltGr keystrokes when the focused element accepts text, so
  typing `|` on a Spanish layout in the composer is unaffected; the chord is Quill-specific
  (tdesktop has no such binding) and only fires outside text input.
- `reg query` replacement in media_tools.rs is out of this brief's file list.

## Verification

gate.sh: GATE OK (core 4808, ui 907). Unit tests for the sanitizer (reserved names, trailing
dots, extension kept, byte cap), XDG_CONFIG_HOME, tilde expansion, desktop id, manifest contents,
and create_new de-dup (a race cannot be unit-tested; the test covers existing-file collisions).

Compile/inspect-only: the actual embedding of the manifest into quill.exe (Windows-only
`windows_icon` path in build.rs, `1 24` resource syntax, forward-slash paths as for the icon) was not
built on Windows; platform CI does not run on PRs. The Linux shell/installer changes and the
GNOME/KDE grouping were not run on a Linux desktop. A dumpbin/`mt.exe -inputresource` check on a
Windows build is the remaining manual step.
