# Changes made by the Quill project

This directory is gpui-pre-wgpu 0.3.14 from crates.io (Apache-2.0,
`LICENSE-APACHE`), GPUI's wgpu renderer and cosmic-text text system (Linux),
used through `[patch.crates-io]` in Quill's `Cargo.toml`. The commit "Vendor the unmodified gpui-pre 0.3.14 registry copy of gpui-pre-wgpu" added
the unmodified registry copy (without the registry's `.cargo-ok`,
`.cargo_vcs_info.json`, `Cargo.toml.orig` and `Cargo.lock`), so
`git diff <that commit> -- third_party/gpui-pre-wgpu` shows every change. The crate's
own unit tests read fonts from the Zed source tree and do not build here.

0.3.14 note: upstream draws opaque quads first in a depth pre-pass. A frame
with glass skips it (a lens batch ends the render pass, and opaque quads drawn
ahead would show in the backdrop the lens copies); the quad shader culls
opaque rectangles only after that pass ran. The pass a lens batch resumes
keeps a depth attachment, and the lens pipeline declares one. Upstream now
has its own `write_instance_data` split, which replaces Quill's. Paths are
tiled curves upstream, so the old `Path`/`ScaledPixels` imports went.
`WgpuRendererCore::target_format` is upstream's field for the headless test
renderer; Quill keeps it in every build, since the frame capture and the lens
pipelines read it.

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
  back as one sequence. The order lives in `install_once_built`, which
  `try_recover` calls with the renderer build and the slot write. Test:
  `a_context_the_renderer_failed_on_is_not_installed` (a failed build leaves
  the slot empty and the window recreating, its failure counted). Not run:
  a real renderer build failing on a working device, and a GPU loss.
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
  While a look runs, `hardware_probe_at` gives the instant of the next
  check for its answer (`HardwareProbe::next_at`), which stays put until a
  check moves it. It used to give the past instant the look started at, so
  every draw armed a timer that fired at once and polled again, one probe
  step a frame instead of one each 250 ms. Test:
  `draws_during_a_look_ask_for_the_armed_check`.
- A GPU that keeps dying is not recovered onto forever (sweep bundle B13,
  L10): `SharedRecovery` counts the shared context's losses (once per
  context, by the window that sees it lost first) over 10 minutes. From the
  3rd, recovery makes the new context on a CPU adapter only
  (`Adapters::SoftwareOnly`, which `select_adapter_and_device` applies the
  way it applies hardware-only) and does not look for the GPU again. It
  gives up (`on_device_gone`: Quill's alert and exit) when no CPU adapter
  configures, or when that forced CPU context is lost too. Before, each
  successful recovery started afresh, so a card or driver that failed again
  after every recovery flashed the windows through recoveries for the rest
  of the session, and the probe moved them back onto it. A first version of
  this fix only allowed a CPU adapter, so the sort still put the dying GPU
  first. `Target::Recreate` and `Attempt::Recreate` now carry `Adapters`
  (hardware only, any, software only), and `new_rejecting_software` became
  `WgpuContext::new_on`. Tests:
  `a_gpu_that_keeps_dying_is_left_for_a_cpu_adapter`,
  `a_keeps_dying_attempt_takes_the_cpu_adapter_listed_after_the_gpu`,
  `losses_far_apart_are_each_recovered_from_afresh`.

The Quill project also changed it in 2026 so Linux draws NV12 video pictures
on the GPU, as gpui-pre-windows does (`docs/decisions/video-nv12-linux.md`):

- `src/yuv_surfaces.rs` (new): `PrimitiveBatch::Surfaces` is drawn. Each
  `SurfaceFormat::Nv12` frame is uploaded into an `R8Unorm` luma and an
  `Rg8Unorm` chroma texture, kept per clip (at most 64 pairs, one per draw)
  and filled again in place while the size stays the same. A pair drawn in
  the current frame is never replaced, so the slot each draw recorded keeps
  naming its own textures; with none to replace, the frame is refused. A
  pair not drawn in a frame is freed at its end, since an idle window may
  render no later frame to free it in. Each draw's
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
  spare (at most two, for eight frames, and only while a texture of its
  size and format is still in use) and taken by the next picture of
  that size, so a playing 1080p clip alternates between two textures instead
  of creating one per picture. Test: `video_pictures_take_turns_in_two_textures`;
  the upstream `before_frame_skips_uploads_for_removed_texture` is now
  `a_tile_removed_before_the_frame_is_safe`.



The Quill project also changed it in 2026 so text glyphs can be swept: the atlas
forwards `text_glyph_count` and `remove_text_glyphs_unused_since` to its
`AtlasState` (see gpui-pre's `QUILL-CHANGES.md`).

## Right-to-left lines (RTL sweep)

Changed by the Quill project in 2026 (`docs/decisions/rtl-sweep.md`):
`src/cosmic_text_system.rs` hands a line's glyphs over left to right on screen
(a stable sort by x). cosmic-text lists a right-to-left line from its right end,
which every `LineLayout` consumer read backwards.

## Backdrop glass

Changed by the Quill project in 2026 (`docs/decisions/backdrop-glass.md`).

- Stage 1: `src/wgpu_renderer.rs` drew `PrimitiveBatch::Lenses` as each
  lens's fallback quad (`BackdropLens::fallback_quad`) through the quad
  pipeline, from a `lens_fallbacks` instance binding.
- Stage 2, real glass, as gpui-pre-apple's `metal_renderer/lens.rs` draws it:
  - `src/wgpu_renderer/lens.rs` (new): a lens batch ends the frame's pass,
    copies each footprint out of the target into a pooled five-level atlas
    (`copy_texture_to_texture`), fills the levels with dual-Kawase
    downsamples, resumes the pass with `LoadOp::Load` and draws the lenses.
    The pipelines are built on the first frame with glass (again if the
    window's alpha mode changes), inside validation and internal error
    scopes; when they fail, glass draws flat for the rest of the renderer's
    life instead of failing every frame. The atlas goes after the first frame
    without glass and on a resize; everything goes with the other resources
    on a device loss. Footprints that don't fit a 16384 atlas, a target
    without `COPY_SRC` and the WebGL instance transport draw the fallback
    quads.
  - `src/shaders_lens.wgsl` (new, after `shaders.wgsl` and
    `shaders_storage.wgsl`): `vs_lens` / `fs_lens`, a line-by-line port of
    `lens_fragment` in gpui-pre-apple's `shaders.metal`. Its bindings are
    group 1's 4 to 7, clear of the surfaces' 0 to 3. `lens.rs` asserts the
    `BackdropLens` offsets that WGSL's vec2 alignment could move.
  - `src/shaders_lens_post.wgsl` (new): the downsample pass and the frame copy.
  - `src/wgpu_renderer.rs`: the surface asks for `COPY_SRC` whenever its
    capabilities offer it (Vulkan, as a rule). A surface without it (OpenGL)
    gets frames with glass drawn into a readable texture of its size, which a
    full-screen pass copies onto it (`glass_frame`, `copy_frame`); frames
    without glass draw straight to it. `render_frame` and `record_frame`
    take the target texture instead of a view (the glass copies from it);
    `write_instance_binding` writes through the new `write_instance_data`,
    which the glass uses for its storage slices. The headless target lost its
    unused view.
- Stage 3: `src/wgpu_renderer/lens.rs` packs with gpui-pre's shared
  `LensPacking::pack` (and `LENS_ATLAS_LEVELS`) instead of its own copy; the
  packing tests moved with it.
- Stage 5: `src/wgpu_renderer/lens.rs` and `src/wgpu_renderer.rs` draw
  from gpui-pre's frame plan: a lens batch copies and blurs only its fresh
  slots, a batch of reused slots does not end the render pass, and the
  plan is kept only after a submit in which every batch drew as planned.
  `src/shaders_lens.wgsl`: gooey shapes, frost towards the rim and the
  magnifier's finer read, as in gpui-pre-apple's `shaders.metal`.
  `src/wgpu_atlas.rs`: `WgpuAtlas::removals`. Inked labels draw in their
  own colour here (the sprite shader ignores the ink) and re-rendered
  backdrops copy the frame like any lens.

## Rounded content masks

Changed by the Quill project in 2026
(`docs/decisions/rounded-content-masks.md`). `src/shaders.wgsl`,
`shaders_webgl.wgsl` (its texture decoder reads the wider instances),
`shaders_subpixel.wgsl`, `shaders_lens.wgsl`: `ContentMask` holds a
`RoundedMask`; the fragment stages multiply their coverage by it, and a
quad, underline, polychrome sprite or path tile with a rounded mask leaves
its fast return and the opaque pre-pass; `QUILL-LAYOUT` lines state each
struct's size and mask offset. `src/yuv_surfaces.rs`: the NV12 surface
parameters carry the shape (112 bytes). `src/wgpu_renderer.rs`,
`src/wgpu_renderer/lens.rs`: `ContentMask::rect`, the instance strides and
the lens layout asserts.

## Snapshots

Changed by the Quill project in 2026 (docs/decisions/snapshot-transitions.md):
the renderer skips a `PrimitiveBatch::Snapshots` batch. Windows on this
backend don't support snapshots yet, so the scene never holds one.
