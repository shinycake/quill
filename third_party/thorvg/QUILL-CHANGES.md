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
Percentages that become 8-bit values and 8-bit file values are clamped, as is a
glyph's size. A dash array keeps its first 64 entries.

`src/loaders/lottie/tvgLottieParserHandler.cpp`: an integer written as a float
reads as that number, and one past an `int` stops inside its limits, so a field
read as a number minus one stays in range.

`src/loaders/lottie/tvgLottieBuilder.cpp`: a composition builds once and is used
only when finished, and only a composition asset can back a precomp. A matte chain
holds at most 16 links, which also ends a chain that loops. A layer's transform
walks its parents in a loop. Star points, polygon points, zigzag ridges and
repeater copies are limited to 1,024 where they size a loop, and the product of a
list's repeaters to 4,096 copies. Text with no document or no font name draws
nothing, and a font with no name is passed over. The small caps buffer lives as
long as the glyph search that reads it, and a glyph draws only the paths in its
groups.

`src/loaders/lottie/tvgLottieModifier.cpp`, `tvgLottieModifier.h`: a path with
nothing to modify stays as it is: rounding leaves fewer than three points alone,
and an offset closes only what it opened and reads a corner's second point only
when it exists. Rounding reads a polystar's layout once, and a second pass treats
the result as a plain path. An offset adds no more than eight times its input's
points by splitting curves.

`src/loaders/lottie/tvgLottieModel.cpp`, `tvgLottieModel.h`: a precomp layer's
children come only from its asset, groups carry a flag for the time they build,
a gradient holds the fewest stops any of its keyframes has, and optional fields
have defaults.

`src/loaders/lottie/tvgLottieCommon.h`: path point and command counts are 32 bits.
A text document's optional fields have defaults. Color steps clamp as floats.

`src/renderer/cpu_engine/tvgSwUtil.cpp`: coordinates are limited to 2^20 px before
they become fixed point, the range the rasterizer's sums and differences can hold.

`src/renderer/cpu_engine/tvgSwRle.cpp`: lines and curves beside the visible cells
skip the cell by cell walk, with the same coverage. The curve flatness tests use
64-bit products. A shape that overflows the cell pool grows it, up to 64 MiB,
before it splits into bands, and an outline of 1,024 points or more keeps a
row's cells in lists by column range. Both only change how cells are found.

`src/renderer/cpu_engine/tvgSwStroke.cpp`: a curve stops splitting at the step a
float can resolve at its coordinates, which is the old step for anything under
2^16. Border arrays grow geometrically.

`src/renderer/cpu_engine/tvgSwShape.cpp`, `tvgSwCommon.h`: at most 8,192 dashes
are cut from one path.

`src/renderer/cpu_engine/tvgSwPostEffect.cpp`: the blur radius is limited to a
kernel past any canvas.

`src/renderer/tvgPaint.cpp`: a clip rectangle's corners stop at the limits of an
`int`.

`src/common/tvgMath.h`: conversions of file values to 8-bit colors are clamped
first.

`src/common/tvgMath.cpp`: a curve's length splits at most eight levels deep.
