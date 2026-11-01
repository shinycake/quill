# Changes made by the Quill project

This directory is the part of ThorVG that `scripts/vendor-thorvg.sh` copies from
the tag in `VERSION`, with `native/patches/thorvg-sticker-limits.patch` applied.
That patch is every change to ThorVG's own files, and the script applies it again
on each bump. Changed code is marked `QUILL`. This file stays across a bump.
Background: `docs/decisions/thorvg-sticker-renderer.md`.

Sticker files come from any chat. `src/sticker_playback/lottie_check.rs` checks a
document's structure before ThorVG reads it. These changes hold the same limits
for what only exists once a frame is evaluated, make a value of the wrong type end
the parse, as in rlottie, make a load read the document to its end, and define the
loader's handling of optional fields and odd values. Real stickers stay far below
every limit here. Their pixels do not change, apart from the text that the parser
below no longer reads.

## Changes

The patch holds the detail of each change. This list says what it touches.

`src/loaders/lottie/tvgLottieParser.cpp`: reading of keyframes, properties, layers
and metadata. Slots, text, fonts and glyphs are not read, since Quill overrides no
slot and rlottie draws no text, so tdesktop shows none. Metadata Quill never uses is
skipped, as rlottie does.

`src/loaders/lottie/tvgLottieParserHandler.cpp`, `tvgLottieParserHandler.h`: number
reading and the way a parse ends.

`src/loaders/lottie/tvgLottieBuilder.cpp`: scene building, with limits on counts that
size a loop.

`src/loaders/lottie/tvgLottieModifier.cpp`, `tvgLottieModifier.h`: the path modifiers.

`src/loaders/lottie/tvgLottieModel.cpp`, `tvgLottieModel.h`, `tvgLottieCommon.h`: the
model's handling of layers, groups, gradients and masks, and its integer widths.

`src/renderer/cpu_engine/tvgSwUtil.cpp`, `tvgSwRle.cpp`, `tvgSwStroke.cpp`,
`tvgSwShape.cpp`, `tvgSwCommon.h`, `tvgSwPostEffect.cpp`: numeric limits in the
rasterizer, the stroker, dashing and blur. A shape that fills the cell pool grows it,
up to 64 MiB, before it splits into bands, and the pool goes back to 4 MiB after that
shape. An outline of 1,024 points or more keeps a row's cells in lists by column
range. Both only change how cells are found.

`src/renderer/tvgPaint.cpp`, `src/common/tvgMath.h`, `src/common/tvgMath.cpp`: numeric
limits in clipping, color conversion and curve length.
