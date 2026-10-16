# Liquid glass for macOS menus (shelved)

Shelved by Idan on 2026-10-16: too much tech debt and work for the value. This
branch keeps the progress. It is not merged and must not be applied to the app
as is.

## What exists

- `third_party/gpui-pre`, `gpui-pre-apple`: a `BackdropBlur` scene primitive
  and a Metal pass. When the draw order reaches a glass region, the pass ends
  the encoder, blits the drawable to a snapshot, runs an MPS gaussian, and
  redraws the rounded rect with a lens rim and a specular edge. Frames without
  glass pay nothing. Ported from crabtalk/zed (Apache-2.0, 693d6a82) and bezel
  (MIT). Credits are in the QUILL-CHANGES.md files and THIRD_PARTY.md.
- `third_party/gpui-component/src/glass.rs`: `GlassProvider` global,
  `GlassStyle::regular` (macOS 26 Regular, fitted to Apple), `GlassSurface`,
  and the PopupMenu hook.
- `src/glass.rs`, `src/ui/glass_watch.rs`, `src/ui/appearance/glass_*`: the
  "Glass effects" setting (macOS only, default on), Reduce Transparency and
  Increase Contrast read from NSWorkspace and observed live, and `GlassMode`.
- `src/ui/demos/glass_lab.rs`: a demo with draggable glass panels over a
  backdrop directory. "b" cycles the backdrop, "f" runs the flip test.
- `tools/glassref/`: GlassRef, a native NSGlassEffectView reference app that
  captures itself through ScreenCaptureKit (`--capture`, `--hdrprobe`, and the
  "f"/"g" flip test). MakeBackdrops generates the structured test backdrops.

## Findings

- ScreenCaptureKit captures of Apple's glass are faithful. Idan confirmed this
  with the live flip test. No pixel exceeds SDR, even in HDR capture mode.
- Apple Regular is heavily blurred, with luminance that adapts to the
  backdrop. Apple Clear is nearly sharp, mirrors content in a thin band along
  the edge, and warps it at the corners.
- Mean abs error against Apple's captures (out of 255, interior / rim, six
  wallpaper spots):
  - Before: 47/40 dark, 37/42 light.
  - Apple-fitted transfer without the readability hold: 3/6 dark, 4.5/8 light.
  - With the hold: 27/24 dark, 14/18 light.
- Apple's own glass fails a 3:1 floor for fixed grey secondary text: about
  1.7:1 dark and 1.4:1 light. Idan chose option B: Apple-light glass, with
  on-glass label colours bright enough to keep 4.5:1 and 3:1.

## Left undone

- Option B is not implemented. The readability hold in `GlassStyle::regular`
  is still option A.
- `src/glass.rs` still carries an obsolete `Recipe` table. Its readability
  test targets that table, not the shipped transfer.
- The provider glue is not written. Nothing installs `GlassProvider`, so the
  app shows no glass.
- The accent highlight pill (the user's Quill accent, inset 5pt, radius 7pt)
  is missing, as are the edge mirroring and corner warping, the Clear variant
  for media overlays (phase 2), a frame profile, and full verification
  captures.
