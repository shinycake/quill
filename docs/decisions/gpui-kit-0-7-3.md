# gpui-kit 0.7.3 (GPUI snapshot gpui-pre 0.3.9)

gpui-kit and gpui-base go from `=0.7.1` to `=0.7.3`, and the GPUI snapshot
crates from gpui-pre 0.3.8 to `=0.3.9`. 0.7.2 and 0.7.3 were cut from
upstream's `stable` branch: 0.7.2 takes the compatible PRs merged since
0.7.1 (`longbridge/gpui-kit` #3458 lists them), and 0.7.3 is 0.7.2 plus
the gpui-pre 0.3.9 upgrade (#3460). Upstream will yank 0.7.2, so Quill
skips it.

## Vendored crates

Quill patches six crates through `[patch.crates-io]`: gpui-base, gpui-pre,
gpui-pre-apple, gpui-pre-macos, gpui-pre-linux and gpui-pre-windows. Each
one moved the same way as in `codex-gpui-kit-0.7.1.md`:

1. Quill's patch is the diff between the old pristine commit and main.
2. A pristine commit adds the new registry copy as is, without
   `.cargo-ok`, `.cargo_vcs_info.json`, `Cargo.lock` or `Cargo.toml.orig`.
   gpui-pre leaves out `examples/` and their `[[example]]` tables, as
   before. gpui-pre-apple now includes `vendor/gpui` in the same commit.
3. A separate commit three-way merges the patch onto it, with the old
   pristine as the base, and fixes what upstream moved.

Each crate's `QUILL-CHANGES.md` names its new pristine commit, so
`git diff <commit> -- third_party/<crate>` still shows the whole patch.

| Crate | Pristine | What had to change |
|---|---|---|
| gpui-base 0.7.1 → 0.7.3 | `1fdaf93c` | `tooltip.rs`: 0.7.2 adds `TooltipDefaults` with a configurable `grace_period` (300 ms) that keeps the tooltip on screen after the pointer leaves. Quill's patch still hides it at once and keeps only the "recently shown" state for the grace period, now read from `TooltipDefaults`. The input engine patch (bidi, formatting spans, inline tokens) merged cleanly over 0.7.2's input fixes, including `text_selection.rs`, which upstream no longer writes on every frame. |
| gpui-pre 0.3.8 → 0.3.9 | `cfc4f7e7` | On Apple platforms 0.3.9's `PaintSurface` and `surface()` take a `CFRetained<objc2_core_video::CVPixelBuffer>` instead of the `core-video` crate's type. Quill's `SurfaceFrame` (CPU pixels for HDR stills on Windows) is cfg'd to non-Apple platforms and didn't change. `surface()` keeps building on every platform. |
| gpui-pre-apple 0.3.8 → 0.3.9 | `09fbdce6` | The Metal renderer moved from `core-video` to `objc2-core-video`. Quill's HDR surface path was ported to it: `CVMetalTextureCache::create_texture_from_image`, `CVMetalTextureGetTexture`, and the headroom read through `CVBuffer::attachment` (`CVBufferCopyAttachment`, so the value is owned). The YUV path now skips a surface CoreVideo can't make textures for, where 0.3.9 asserts on the status and `expect`s a texture. 0.3.9 also moved the CoreText text system here from gpui-pre-macos, so the malformed-run fix in `layout_line` moved with it. |
| gpui-pre-macos 0.3.8 → 0.3.9 | `d8857682` | `display_link.rs`: the present-time code reads host time through `objc2_core_video::CVGetCurrentHostTime` and `CVGetHostClockFrequency`, like 0.3.9's `frame_request_instant`. The idle display link, notification extras and keyboard fix merged cleanly. Two patches were dropped: 0.3.9 fixed `MacDisplay::all` upstream (it logs and returns no displays when `CGGetActiveDisplayList` fails), and `text_system.rs` is no longer in this crate. |
| gpui-pre-linux 0.3.8 → 0.3.9 | `c3a094d0` | Merged cleanly. |
| gpui-pre-windows 0.3.8 → 0.3.9 | `be35a01b` | 0.3.9 takes `WM_USER + 10` for `WM_GPUI_DISPLAYS_CHANGED`, which was Quill's `WM_GPUI_DESTROY_AFTER_DEVICE_LOST`. Quill's two messages move to `WM_USER + 11` and `+ 12`. 0.3.9 also trimmed the `windows` feature list, so Quill's `Foundation_Collections` and `Win32_Devices_Display` are added to the new list. `events.rs` and `platform.rs` conflicted only around those constants: 0.3.9 also handles `WM_ENDSESSION` in the platform window itself, so `WM_GPUI_END_SESSION` is gone upstream, and Quill's `handle_gpui_events` keeps returning `None` for an unknown message instead of 0.3.9's `unreachable!()`. |

Upstream fixed one of Quill's patches (the display list panic). Every other
patch is still needed: the macOS display link, the X11 refresh timer and the
Windows vsync thread still run at the refresh rate for as long as a window
is visible, the input engine still has no bidi support, and the 0.3.9
renderer still asserts on any surface that isn't bi-planar video. The two
`frame_idle.rs` copies (X11 and Windows) didn't change and are still
identical.

## API changes in Quill

- **CoreVideo types.** GPUI's `surface` takes a
  `CFRetained<objc2_core_video::CVPixelBuffer>` on macOS. The AVPlayer
  backends (`ui/native_video.rs`, `ui/video_player/backend_avplayer.rs`),
  the rotate path (`ui/native_video_rotate.rs`) and HDR stills
  (`ui/hdr_image/macos.rs`) now use `objc2-core-video` and
  `objc2-core-foundation` throughout. `copyPixelBufferForItemTime` already
  returns an owned buffer, so the `wrap_under_create_rule` helpers are gone.
  Quill no longer depends on the `core-video` or `core-foundation` crates
  directly. `objc2-core-video` gains the `objc2` and `CVHostTime` features,
  and `objc2-core-foundation` gains `objc2`.
- **Interface zoom.** 0.3.9 adds `Platform::on_displays_changed` and
  `PlatformWindow::on_display_changed` with default bodies.
  `forwards_every_trait_method` caught both, and `ZoomPlatform` and
  `ZoomWindow` now forward them. That test used to read GPUI's
  `platform.rs` from the cargo registry and silently skipped when it wasn't
  there. gpui-pre is vendored now, so the registry has no copy, and the
  test reads `third_party/gpui-pre/src/platform.rs` instead. It can no
  longer skip.
- **Lockfile.** `cocoa` goes from 0.26.1 to 0.26.0 and `unicode-properties`
  from 0.1.4 to 0.1.3. 0.7.2's optional `gpui-fast` backend crates pin both
  exactly, and Cargo.lock includes optional dependencies whether or not a
  feature enables them. Quill doesn't enable `gpui-fast`, so those crates
  are in the lockfile but aren't built (`taffy` 0.14 comes in the same
  way). New in the build: `similar` (the kit's Diff component), and on
  macOS `objc2-security`, which gpui-pre-apple now uses with the text
  system it took over from gpui-pre-macos. gpui-pre-apple also uses
  `mach2` 0.6, which cpal 0.18 already brought in.
- Everything else compiled unchanged, including main's newer passkey code.

## Free fixes

These need no Quill code:

- Context menus are focused before their first frame, and an input's
  context menu keeps the selection and focus ring (#3422, #3382). This
  applies to the composer and every text field.
- Auto-grow inputs stay fitted when their row limits change (#3430). The
  composer is an `auto_grow` textarea.
- Completion and code-action popups follow the caret of the current
  frame (#3412).
- `text_selection` no longer writes selection state on every frame
  (#3407), and text and resizable state stop notifying when nothing
  changed (#3418). Both mean fewer redraws.
- Tabs scroll a clicked tab into view (#3419), and dock hands focus to the
  tab that replaces a closed one (#3409).
- gpui-pre 0.3.9: `MacDisplay::all` no longer panics, and each display
  reports a `refresh_interval` (ProMotion's maximum on built-in panels).

## New features

### Not adopted: the gpui-fast backend

0.7.2 adds an optional `gpui-fast` feature that builds the kit on the
`gpui-fast` crates instead of gpui-pre. Quill's six patches target
gpui-pre, so switching would mean vendoring and re-patching a second GPUI.
Left out.

### Not adopted: Diff, configurable dialog entrance, `InputState::add_selection`

Quill shows no patches. tdesktop's boxes don't animate in differently per
dialog. The composer has no multi-cursor editing. Long-press context
menus only matter on touch screens.

### Not adopted yet: display observers

0.3.9 adds `App::observe_displays` (displays added, removed or changed) and a
per-window display observer. tdesktop rebuilds its "Display for
notifications" choice when a screen is plugged in or out. Quill reads
`cx.displays()` each time it renders that setting and each time it places
a notification, so it is already correct when the page is reopened. See
Follow-ups.

## Follow-ups

- Settings → Notifications → "Display for notifications" could refresh
  while open when a display is added or removed (`App::observe_displays`).
- The IME candidate window under interface zoom on macOS and Windows is
  still open (see `codex-gpui-kit-0.7.1.md`).

## Verification

In a Linux cloud container (x86_64, no GPU):

- The full gate passes: app-name, file-size, language and hotspot checks,
  `cargo fmt`, clippy (`--no-default-features` and `demo-capture`, both
  `-D warnings`), core tests, UI tests (`--features demo-capture --bin
  quill`, including `forwards_every_trait_method` against 0.3.9),
  `scripts/test-frame-idle.sh` (9 tests), `scripts/test-notification-extras.sh`
  (3 tests), `cargo deny check licenses`, and
  `scripts/third-party-licenses.sh --check`. The one core test failure,
  `atomic_write_failure_keeps_old_file`, happens only because the container
  runs as root. It passes on CI. `cargo deny` now warns that the
  `bzip2-1.0.6` allowance in `deny.toml` matches nothing. That's a warning,
  not a failure, and it is left for whoever next trims the allow list.
- Each pristine commit matches its crates.io archive, apart from the files
  left out on purpose.
- Cross `cargo check --target aarch64-apple-darwin` of gpui-base, gpui-pre,
  gpui-pre-apple and gpui-pre-macos, and of Quill (lib, bin and tests),
  passes with no warnings. `cargo clippy -D warnings` on Quill for that
  target is also clean. The container has no Xcode, so two stand-ins were
  used that are not committed: a C compiler that writes empty objects
  (`cargo check` never links), and a gpui-pre-apple `build.rs` stub that
  writes empty shader outputs instead of running `xcrun metal`. The Metal
  shaders (including Quill's `hdr_surface_fragment`) were not compiled, and
  nothing ran on macOS.
- Cross `cargo check --target x86_64-pc-windows-msvc` of Quill (lib, bin and
  tests) passes with the same stand-in C compiler, and
  `cargo clippy -D warnings` of the gpui-pre-windows library is clean.
  Quill's warnings on that target are all in
  files this change doesn't touch (`winreg.rs`, `local_path.rs`,
  `connect/tests/passkeys_login.rs`, `ui/image_decode/tests.rs`, and
  `ErrorKind::Open`, which Linux also reports). The HLSL shaders were not
  compiled (no `fxc`), and nothing ran on Windows. gpui-pre-windows' own
  `lib test` target doesn't build, and neither does the unmodified registry copy's: upstream's
  `render_to_image` is cfg'd on `test`, but the trait method only exists
  with `test-support`.
- Visual check: debug `demo-capture` builds of main and of this branch,
  under Xvfb with Mesa's software Vulkan at 1400x900, `QUILL_REDUCED_MOTION=1`.
  The screens were `ready-chats`, `ready-chats-composer`,
  `ready-chat-list-search`, `ready-settings-advanced`, `ready-privacy` and
  `ready-profile-edit`, each in light and dark. All 12 pairs are pixel
  identical (`compare -metric AE` = 0).
- Idle CPU on the same runs: the process's CPU time over 10 s, starting
  3 s after the window was ready. The idle-frames patch still parks X11
  windows. Five screens read 0.0 to 0.2% on both builds.
  `ready-profile-edit` reads 22.6 to 22.9% on both builds, so that cost is
  in the fixture, not in this change.

| Screen (light / dark) | main, %CPU | this branch, %CPU |
|---|---|---|
| ready-chats | 0.0 / 0.0 | 0.1 / 0.1 |
| ready-chats-composer | 0.0 / 0.0 | 0.1 / 0.0 |
| ready-chat-list-search | 0.1 / 0.1 | 0.1 / 0.0 |
| ready-settings-advanced | 0.2 / 0.1 | 0.0 / 0.1 |
| ready-privacy | 0.0 / 0.1 | 0.0 / 0.1 |
| ready-profile-edit | 22.9 / 22.6 | 22.8 / 22.8 |

Not verified:

- Anything on macOS or Windows at runtime. That covers video through the
  ported `objc2-core-video` paths (AVPlayer frames, rotation, the
  `QuillHdrHeadroom` attachment and HDR stills), the macOS display link and
  present timing, and the Windows device-loss messages at their new ids.
  These are type-checked only and should be tried on a Mac and a Windows
  machine before release.
- The patched gpui-base unit tests, including the tooltip test. gpui-base
  isn't a workspace member, so `cargo test -p gpui-base` can't run here.
  Quill's own tests exercise the tooltips and the input engine.
