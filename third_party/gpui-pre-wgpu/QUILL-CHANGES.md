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
  8 s, a software adapter is accepted after 3 hardware-only failures (at
  once when the lost device was already a CPU one), and after 8 failures
  (about 35 s) the renderer gives up.
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
