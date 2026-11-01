# Changes made by the Quill project

This directory is gpui-pre-wgpu 0.3.10 from crates.io (Apache-2.0,
`LICENSE-APACHE`), GPUI's wgpu renderer and cosmic-text text system (Linux),
used through `[patch.crates-io]` in Quill's `Cargo.toml`. Commit `6be84cd7` added
the unmodified registry copy (without the registry's `.cargo-ok`,
`.cargo_vcs_info.json`, `Cargo.toml.orig` and `Cargo.lock`), so
`git diff 6be84cd7 -- third_party/gpui-pre-wgpu` shows every change. The crate's
own unit tests read fonts from the Zed source tree and do not build here.

The Quill project changed it in 2026 so emoji draw in colour on Linux
(`docs/decisions/linux-titlebar-emoji.md`):

- `src/cosmic_text_system.rs`: cosmic-text's fallback list reaches GNU FreeSans
  and DejaVu Sans before Noto Color Emoji. FreeSans has boxed letters for the
  regional indicators, so a flag such as 🇺🇸 drew as two boxed letters; it and
  DejaVu also have outline glyphs for many emoji (❤, ⛰, keycaps, 😂).
  `layout_line_no_separators` now gives every grapheme that asks for emoji
  presentation (Unicode TR #51: emoji-default characters, U+FE0F, keycaps,
  flags, skin tones, ZWJ and tag sequences; U+FE0E keeps text) its own span
  that names Noto Color Emoji, when that family is installed, so the whole
  cluster shapes in colour. Text-default symbols (©, ™, a bare ❤) keep their
  text glyphs.
- `Cargo.toml`: `unicode-properties`, for the emoji presentation tables.

The Quill project also changed it in 2026 to give back memory after
browsing (`docs/decisions/sweep-r3-browse-memory.md`):

- `src/wgpu_atlas.rs`: implements `AtlasBackend::insert_image` (see
  gpui-pre's `QUILL-CHANGES.md`). Each `WgpuAtlasTexture` records whether
  it holds images, and `allocate` only places a tile in a texture holding
  the same kind, so an image texture is freed once its images leave.
  Test: `glyphs_never_keep_an_image_texture_alive`.

The Quill project also changed it in 2026 for Agent access's screenshots
(`docs/decisions/agent-access.md`, phase 5 step 3):

- `src/wgpu_renderer.rs`: `WgpuRenderer::capture_scene` renders a scene to
  an offscreen texture in the surface format (BGRA8 or RGBA8; other formats
  answer with an error), copies it to a mapped buffer with 5 s timeouts and
  returns RGBA rows. Not built for wasm. It renders in the surface's alpha
  mode (the pipelines blend for it), so the rows are premultiplied in both
  the opaque and the premultiplied mode.

The Quill project also changed it in 2026 so color emoji glyphs can be swept
(`docs/decisions/idle-wallpaper-and-emoji-trim.md`):

- `src/wgpu_atlas.rs`: `WgpuAtlas` forwards `PlatformAtlas::next_use_mark`,
  `emoji_glyph_count` and `remove_emoji_glyphs_unused_since` to its
  `AtlasState` (see gpui-pre's `QUILL-CHANGES.md`).

The Quill project also changed it in 2026 so small pictures don't keep big
textures (`docs/decisions/images-left-behind.md`):

- `src/wgpu_atlas.rs`: tiles are sorted into `TileClass`es and small
  images get 512² textures, the same change as gpui-pre-apple's
  `metal_atlas.rs`. Tests: `small_pictures_keep_to_textures_of_their_own`,
  `animation_frames_rotate_through_one_texture`; the tile-reuse test's
  keeper tile is 200². The atlas tests ran on lavapipe, through
  `cargo test -p gpui-pre-wgpu --lib wgpu_atlas` with the `test-support`
  feature on and the crate's other test modules left out (they read fonts
  from the Zed tree or need `naga`).

The Quill project also changed it in 2026 so a GPU that never comes back no
longer keeps the UI thread asleep (sweep finding linux-windows-3):

- `src/device_recovery.rs` (new, std-only; tests run with
  `scripts/test-device-recovery.sh`): when to try again after a device loss.
  The first attempt waits 350 ms (upstream's sleep), later ones back off to
  8 s, a software adapter is accepted after 7 hardware-only failures (the
  last at about 27 s; at once when the lost device was already a CPU one),
  and after 9 failures (about 43 s) the renderer gives up.
- A window that recreated the context from a lost GPU and landed on a CPU
  adapter looks for the GPU again (`HardwareProbe`: after 10 s, then less
  often, up to every 2 minutes). `WgpuRenderer::probe_hardware` first asks
  whether a non-CPU adapter supports the window's surface; only then does
  it let go of the window's software swapchain and make a hardware-only
  context, retiring the software one (`DeviceErrorState::retire`) so the
  other windows adopt the new one on their next draw. If that fails, the
  window goes back onto the software context. The caller arms the timer
  from `hardware_probe_at`.
- `src/wgpu_renderer.rs`: `recover` no longer sleeps on the UI thread and
  returns a `Recovery` (`Recovered`, `RetryAt(instant)` or `GaveUp`) instead
  of a `Result`; upstream retried on every frame forever, rejecting software
  adapters so a real GPU returning late after resume is not traded for a CPU
  renderer. Giving up calls the handler set with the new `on_device_gone`
  (Quill shows its launch failure alert and exits). Also drops the
  `expect`s in `recover`.
- A window that only has to adopt the context another window recreated
  goes through the same backoff (`Target::Adopt`, first attempt at once)
  instead of retrying on every draw, and its failures start from zero
  when the context comes back. Giving up there returns
  `Recovery::CloseWindow`: the other windows keep drawing and only that
  one closes, as on Windows. A successful recovery clears the state, so a
  later loss starts afresh. `Recovering` holds this wiring and its tests
  run with the policy's.
- `try_recover` puts a recreated context in the shared slot only after the
  window's renderer is built on it (sweep bundle B13, L09). Before, a build
  that failed after the context was made left it installed: the window's
  next `recover` saw a healthy context, switched to adopting it with its
  failures reset (so the hardware-only and software steps started over),
  and the other windows adopted a context the recreating window could not
  draw with. Now the slot stays empty and the recreation backs off and falls
  back as one sequence. Not run: there is no way to make the renderer build
  fail on a working device here, and no GPU loss to recover from.
- The look for the GPU belongs to the shared context, not to the window
  that recreated it (sweep bundle B13, N45): `GpuContext` is now
  `Rc<GpuSlot>`, the context plus a `SharedRecovery` holding the
  `HardwareProbe`. A recreation that lands on a CPU adapter sets it, an
  adoption keeps it, a loss clears it, and `hardware_probe_at` and
  `probe_hardware` read it from any window. Before, closing the window that
  recreated the context left the others on the software renderer for the
  rest of the session. Test: `any_window_on_the_fallback_looks_for_the_gpu`.
- A look for the GPU runs off the UI thread (sweep bundle B13, L11):
  `look_for_gpu` makes a Vulkan/GL instance with no display and lists the
  adapters on a thread of its own, and the windows check back for its answer
  every 250 ms (`ProbeStep::Await`). Only when it saw a non-CPU adapter does
  the window run `gpu_can_present` (which needs its surface) and recreate.
  Before, every look, every 10 s to 2 minutes for as long as the GPU stayed
  away, made an instance and a surface on the UI thread. Test:
  `a_look_for_the_gpu_runs_off_the_ui_thread`. Not measured: there is no
  display or GPU loss here to time a look against.
- A GPU that keeps dying is not recovered onto forever (sweep bundle B13,
  L10): `SharedRecovery` counts the shared context's losses (once per
  context, by the window that sees it lost first) over 10 minutes. From the
  3rd, recovery accepts a CPU adapter at once and does not look for the GPU
  again; at the 6th it gives up (`on_device_gone`: Quill's alert and exit).
  Before, each successful recovery started afresh, so a card or driver that
  failed again after every recovery flashed the windows through recoveries
  for the rest of the session, and the probe moved them back onto it.
  `Target::Recreate`'s field is now `software_at_once`. Tests:
  `a_gpu_that_keeps_dying_is_left_and_then_given_up_on`,
  `losses_far_apart_are_each_recovered_from_afresh`.

The Quill project also changed it in 2026 so Linux draws NV12 video pictures
on the GPU, as gpui-pre-windows does (`docs/decisions/video-nv12-linux.md`):

- `src/yuv_surfaces.rs` (new): `PrimitiveBatch::Surfaces` is drawn. Each
  `SurfaceFormat::Nv12` frame is uploaded into an `R8Unorm` luma and an
  `Rg8Unorm` chroma texture, kept per clip (at most 16 pairs) and filled
  again in place while the size stays the same; a pair not drawn for two
  frames, and every pair once no surface is on screen, is freed. Each draw's
  bounds, mask and `YuvColor::rgb_rows` go into one uniform buffer at
  dynamic offsets (at most 64 draws a frame). Frames are marked drawn or
  refused (other formats, too large for a texture). Tests run on lavapipe.
- `src/shaders.wgsl`: `fs_surface` takes the matrix rows from the uniform
  instead of a fixed full-range BT.601 matrix and blends like the other
  pipelines.
- `src/wgpu_renderer.rs`: the surfaces layout's uniform has a dynamic
  offset; `record_frame` prepares and draws the surfaces.

The Quill project also changed it in 2026 so pictures shown as images cost
fewer copies and textures (sweep finding media-7,
`docs/decisions/video-nv12-linux.md`):

- `src/wgpu_atlas.rs`: `upload_texture` hands the bytes to
  `queue.write_texture` at once (the queue stages them for the next submit)
  instead of copying them into a pending list that `before_frame` flushed;
  `swizzle_upload_data` returns a `Cow` and copies only to swap BGRA into
  RGBA. A freed texture larger than 1024² (one picture's own) is kept as a
  spare (at most two, for eight frames) and taken by the next picture of
  that size, so a playing 1080p clip alternates between two textures instead
  of creating one per picture. Test: `video_pictures_take_turns_in_two_textures`;
  the upstream `before_frame_skips_uploads_for_removed_texture` is now
  `a_tile_removed_before_the_frame_is_safe`.



The Quill project also changed it in 2026 so text glyphs can be swept: the atlas
forwards `text_glyph_count` and `remove_text_glyphs_unused_since` to its
`AtlasState` (see gpui-pre's `QUILL-CHANGES.md`).
