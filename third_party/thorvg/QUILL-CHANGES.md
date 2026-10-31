# Changes made by the Quill project

This directory is the part of ThorVG that `scripts/vendor-thorvg.sh` copies from
the tag in `VERSION`, with `native/patches/thorvg-sticker-limits.patch` applied.
That patch is every change to ThorVG's own files, and the script applies it again
on each bump. Changed code is marked `QUILL`. This file stays across a bump.
Background: `docs/decisions/thorvg-sticker-renderer.md`.

Sticker files come from any chat. `src/sticker_playback/lottie_check.rs` checks a
document's structure before ThorVG reads it. These changes hold the same limits
for what only exists once a frame is evaluated, and keep the loader's handling
of optional fields and odd values well defined. Real stickers stay far below every
limit here, and their pixels do not change.

## Changes

`src/loaders/lottie/tvgLottieParser.cpp`: the scan for a shape's type reads
strings whole and counts only a key. Property arrays may hold a null. The search
for a shared interpolator looks at the newest 1,024 entries. Slot ids are skipped,
since Quill overrides no slot. A font's and a glyph's family and style are optional.
Percentages that become 8-bit values are clamped.

`src/loaders/lottie/tvgLottieBuilder.cpp`: a composition builds once and is used
only when finished, and only a composition asset can back a precomp. Matte links
stay a simple chain. Star points, polygon points, zigzag ridges and repeater copies
are limited to 1,024 where they size a loop, and the product of a list's repeaters
to 4,096 copies.

`src/loaders/lottie/tvgLottieModel.cpp`, `tvgLottieModel.h`: a precomp layer's
children come only from its asset, and groups carry a flag for the time they build.

`src/renderer/cpu_engine/tvgSwUtil.cpp`: coordinates are limited to 2^20 px before
they become fixed point, the range the rasterizer's sums and differences can hold.

`src/renderer/cpu_engine/tvgSwRle.cpp`: lines and curves beside the visible cells
skip the cell by cell walk, with the same coverage. The curve flatness tests use
64-bit products.

`src/renderer/cpu_engine/tvgSwShape.cpp`, `tvgSwCommon.h`: at most 8,192 dashes
are cut from one path.

`src/renderer/cpu_engine/tvgSwPostEffect.cpp`: the blur radius is limited to a
kernel past any canvas.

`src/common/tvgMath.h`: conversions of file values to 8-bit colors are clamped
first.
