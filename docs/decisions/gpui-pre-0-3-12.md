# gpui-pre 0.3.12

GPUI moves from the gpui-pre 0.3.11 snapshot (zed@f816e1f) to 0.3.12
(zed@d2a4fb6). The six vendored crates (`gpui-pre`, `gpui-pre-apple`,
`gpui-pre-macos`, `gpui-pre-linux`, `gpui-pre-windows`, `gpui-pre-wgpu`) are
re-vendored and patched. gpui-kit stays at 0.9.0. The same round took the 24
compatible `cargo update` bumps (#1324) and ashpd 0.14 for Linux screen sharing.

**Landed with gpui-kit 0.9.1** (2026-11-11). gpui-kit 0.9.0 pinned
`gpui-pre =0.3.11`, so this re-vendor waited on a branch until gpui-kit 0.9.1
moved to 0.3.12. What that release changed, and what Quill took from it, is in
`docs/decisions/gpui-kit-0-9-1.md`. The verification below was done before
then, in a trial with patched kit pins; the landing round was verified on the
real lockfile.

## What tdesktop does

Nothing relevant. This is Quill's own dependency policy
(`docs/dependency-updates.md`).

## What changed upstream

- **Windows color glyphs on the CPU.** DirectWrite color layers are composited
  in Rust (`direct_write.rs`) instead of on the GPU, so `color_text_raster.hlsl`,
  the `EmojiRasterization` shader module and the text system's DirectX devices
  are gone (`DirectWriteTextSystem::new()` takes no devices, no
  `handle_gpu_lost`/`release_gpu`). Tabs keep their native advances. The
  DirectComposition target is created with `topmost = false`.
- **Accessed-entity scopes.** `EntityMap::accessed_entities` is private now, an
  `AccessedEntities` with a frame set and nested scopes. New: `push_access_scope`,
  `pop_access_scope`, `take_accessed`, `restore_accessed`. `extend_accessed`
  takes `impl IntoIterator<Item = EntityId>` and skips released entities, so a
  reused cached view no longer subscribes to entities App already dropped.
- **Cursor after mouse events.** `Window::dispatch_mouse_event` resolves the
  cursor after the listeners on every event, so a drag started by a listener
  shows its cursor at once.
- **Lists.** `content_insets` counts padding and border; horizontal padding
  insets items and is part of the inferred width.
- `Along::apply_along` takes `impl Fn` instead of `impl FnOnce`.
- New: `Anchored::attach(target, corner)`, `Window::rendered_scene`,
  `BenchContext::simulate_resize`, animated WebP decoding next to GIF.
- macOS menu key equivalents for Backspace and Delete use the menu codes
  (`key_to_native` is now `key_to_menu_equivalent`).
- Linux and wgpu: version bumps only.

Renames that only break on Linux or Windows: none reach Quill. `src/` uses
none of `apply_along`, `extend_accessed`, `accessed_entities` or
`key_to_native`, and gpui-base and gpui-component use none either.

## What Quill's patches needed

Each crate has a pristine commit ("Vendor the unmodified gpui-pre 0.3.12
registry copy of <crate>", same exclusions as `1398c875`) and a patch commit
that three-way applies the crate's diff from `1398c875`.

| Crate | What had to change |
|---|---|
| gpui-pre | `entity_map.rs`: the test-support `type_names` map sits next to upstream's `AccessedEntities`. `window.rs`: the drag patch keeps only its `hit_test_changed` flag; upstream now resets the cursor at the end. `window/live_views.rs` (Quill's) uses `take_accessed` instead of the private field. |
| gpui-pre-windows | `ShaderModule` keeps Quill's `HdrSurface` and `YuvSurface` without `EmojiRasterization`. The vsync thread, its backoff and the open-window device recovery lose the text-system argument, since the text system holds no devices. |
| gpui-pre-wgpu | Header comment lines only. |
| gpui-pre-apple, gpui-pre-macos, gpui-pre-linux | Applied cleanly. |

## Other updates in this round

- `cargo update`: the 24 crates in #1324. notify-rust 4.19, under
  gpui-pre-linux, moved to zbus 6.
- ashpd 0.14: `SourceType` and `CursorMode` are bitflags structs now, so
  `src/calls/capture/screen_linux.rs` uses `SourceType::MONITOR` and
  `CursorMode::EMBEDDED`. Nothing else Quill calls changed. ashpd 0.14 is on
  zbus 6; Quill's own zbus stays on 5 with ksni, accesskit_unix and GPUI, which
  still link ashpd 0.13 through gpui-base and gpui-pre-linux. The Linux lockfile
  has two zbus and two ashpd versions; `deny.toml` has no duplicate ban.
- Held: moxcms 0.9, x11rb 0.14 (gpui-pre-linux 0.3.12 still asks for 0.13), zbus 6.

## How it was verified

Without the gpui-kit pin there is no buildable lockfile, so the gpui-pre work was
checked in a local trial that is not committed: a copy of gpui-kit 0.9.0 and
gpui-kit-assets 0.9.0 with their pins at `=0.3.12`, patched in, and the vendored
gpui-base/gpui-component pins at `=0.3.12`. In that trial:

- macOS: `cargo clippy --features demo-capture --all-targets -- -D warnings`
  passes, and `cargo test -p gpui-pre-apple --lib -- metal_renderer::tests` passes
  (11 tests).
- Linux: `cargo check --target x86_64-unknown-linux-gnu --features ui` passes with
  stub C tools (nothing links), which covers gpui-pre-linux, gpui-pre-wgpu and
  `screen_linux.rs` on ashpd 0.14.
- Windows: `cargo check -p gpui-pre-windows --target x86_64-pc-windows-msvc`
  passes with a stub `llvm-rc`. Quill itself does not check for Windows here
  (ring needs a Windows C sysroot).
- Screenshots of `ready-chats`, `ready-item-menus` and `ready-info-group`, light
  and dark, against the same demos built from the 0.3.11 tree: the chat and menu
  captures are byte-identical, and the group info captures differ only by the
  side panel sitting one point further left, most likely where its slide-in
  animation was when the capture ran.

The cargo update and ashpd commits were checked on the real lockfile (gpui-pre
0.3.11): `gate.sh` (GATE OK), macOS
`cargo clippy --features demo-capture --all-targets -- -D warnings`, the
gpui-pre-apple `metal_renderer` tests, and the same clippy for
`x86_64-unknown-linux-gnu` with stub C tools.

Not verified: anything on real Linux or Windows hardware, the Windows color emoji
path (now CPU), Quill's Windows code against 0.3.12, and a gpui-kit release
that pins 0.3.12, which may itself change.
