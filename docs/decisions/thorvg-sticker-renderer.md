# ThorVG sticker renderer

Animated stickers (TGS, gzipped Lottie) render with ThorVG 1.1.3, compiled
into the Quill binary. It replaces Samsung rlottie, which shipped as a
separate library (`librlottie.dylib`, `librlottie.so`, `rlottie.dll`) built by
two scripts with a local patch.

## Why

Measured on 41 real cached stickers at 448 px, every frame, against Telegram
Desktop's own rlottie fork as the reference. A pixel counts as different when
a premultiplied channel moves by more than 24 of 255.

| Renderer | Stickers with > 1% of pixels different | Median ms per frame |
| --- | --- | --- |
| rlottie as shipped (patched ea06d2f) | 12 of 41 | 0.57 |
| ThorVG 1.1.3, CPU, no SIMD | 2 of 41 (1.74% and 1.97%) | 0.71 |
| Telegram's rlottie fork | reference | 0.56 |

The rlottie differences were missing or misplaced shapes; the two ThorVG
ones are antialiasing along long edges. ThorVG costs about 25% more per
frame, which the decode workers absorb at sticker sizes. A NEON build was not
faster, so ThorVG builds without SIMD flags. The benchmark lived in the
sticker-player scratch directory (bench.cpp, rlottie loaded with dlopen, ThorVG
through its C API, one process, Apple M1 Pro).

Other gains: ThorVG renders fractional frames and straight-alpha output, has
tagged releases, and is maintained. Being compiled in removes three native
build jobs, three bundled libraries and their load checks, and the
`QUILL_RLOTTIE_PATH` setup for development builds. Sticker decoding now runs
in the core crate's CI tests on Linux.

## How it is built

- `scripts/vendor-thorvg.sh <tag>` copies a subset of a ThorVG checkout into
  `third_party/thorvg` and writes `VERSION` (tag, commit, version). The subset is
  `inc/`, `src/common`, `src/renderer` with `cpu_engine`, `src/loaders/raw`,
  `src/loaders/lottie` with its RapidJSON copy, and `src/bindings/capi`.
  JerryScript and `tvgLottieExpressions.cpp` stay out. `tvgLottieExpressions.h`
  stays in: with expressions off it holds the no-op stubs the parser calls.
- `build.rs` compiles every `.cpp` of those directories with the `cc` crate into
  a static library: C++14, `-O3` in every profile (debug builds decode at
  release speed and the golden test sees the same pixels everywhere), `NDEBUG`,
  `TVG_STATIC`. GCC and Clang get `-fno-exceptions -fno-rtti -fno-math-errno`,
  as in ThorVG's meson build, but keep the stack protector and unwind tables:
  the input comes from any chat, and crash reports must walk these frames.
  MSVC gets `/EHs-c- /GR- -D_HAS_EXCEPTIONS=0` and `NOMINMAX`.
- The generated `config.h` defines the version, the CPU engine, the Lottie
  loader and the C API. File IO, partial rendering, threads and logging are
  off. `rerun-if-changed` covers `third_party/thorvg` only; cargo caches the
  objects otherwise.

Threads stay off on purpose. Quill decodes on its own workers, one canvas per
decode, and without `THORVG_THREAD_SUPPORT` ThorVG's scheduler runs each task
inline on the calling thread. The state canvases share (renderer count,
memory pool list, engine init) sits behind ThorVG's `StrictKey`, a real
`std::mutex` whenever the compiler defines `__STDCPP_THREADS__`; the generated
`config.h` stops the build if it does not. Data loads with `copy = true`, which
keeps it out of ThorVG's unguarded loader cache. So decodes run concurrently;
rlottie's path serialized them behind one lock. The engine starts once
(`tvg_engine_init` in a `OnceLock`) and is never terminated, since termination
needs every canvas gone and the process exit frees it anyway.

Output is `TVG_COLORSPACE_ARGB8888S`: straight alpha, each pixel the `u32`
`0xAARRGGBB`, written out with `to_le_bytes` as B, G, R, A. That is GPUI's
RenderImage layout (its sprite pipeline blends with `SourceAlpha`), so the
un-premultiply passes in `src/ui/sticker_playback.rs` and `src/ui/editor_art.rs`
are gone. The gift theme stills (`src/ui/peer_qr/gift_art.rs`) had been fed
premultiplied pixels; they are straight now too.

The validation in `tgs_json` is unchanged: 1 MiB file cap, 8 MiB gunzip cap,
size 1 to 4096, frame rate 1 to 120, at most 30 s, no external image assets.

## Skin tones

Emoji animations can carry a top-level `"fitz"` array of
`{"o", "f12", "f3", "f4", "f5", "f6"}` colors (`0xRRGGBB`). Telegram's rlottie
fork swaps each matching color for the chosen tone while parsing.
`apply_fitzpatrick(&mut Value, FitzType)` ports that over the parsed JSON before
ThorVG sees it, following `lottieparser.cpp` call for call:

- Fills (`fl`) and strokes (`st`) recolour their `c`, static `k` arrays and the
  `s` and `e` of every keyframe. Each channel is clamped to 0..1, read as an
  `f32`, scaled by 255 and rounded, then compared as `0xRRGGBB`; the first
  matching entry wins. RGB changes, alpha and anything after it stay.
- Layer solid colors (`sc`, `"#rrggbb"`) recolour the same way.
- Gradient stops do not change, as in rlottie.
- rlottie applies the table while streaming, so only top-level entries after
  `"fitz"` change (`layers`, and `assets[].layers` for precomps). An entry
  without the chosen tone maps to black, as rlottie reads a missing key as 0.

`FitzType::from_tdlib` maps TDLib's `fitzpatrick_type` (1 and 2 share the
light set), as Telegram X's decoder does. When nothing changes the original
bytes go to ThorVG; otherwise the document is serialized again, in the
file's key order (`serde_json` has `preserve_order`), which ThorVG's streaming
parser needs for `"ty"`. The reference tdesktop checkout's `lib_lottie` now
calls a `tlottie` library that is not in the reference repos, so the port
follows tg-rlottie. `decode_tgs_sized` takes the tone; every caller passes
`FitzType::None` until emoji messages pass the real one.

## Golden frames

`tests/sticker_golden.rs` renders five cases at 128 px and compares two
frames of each with PNG references in `tests/fixtures/stickers/golden`. A
pixel is off when a premultiplied channel moves by more than 16; a frame fails
above 0.5% off pixels, which leaves room for compiler rounding at
antialiased edges but not for a changed shape. Failing frames are written
with diff images to the temp directory.

| Case | Covers |
| --- | --- |
| `matte-mask` (hand-written) | track matte, animated subtract mask, animated roundness, stroke |
| `fitz-face` none and type 5 (hand-written) | skin tone on fill, keyframed stroke, solid layer; gradient kept |
| `trim-paths` (cached sticker 440fbc4810c2) | trim paths, 48 animated paths; rlottie was 5.7% off |
| `many-paths` (cached sticker d8f36dcb4076) | 104 animated paths, the face and hand case of `rlottie-shape-budget.md` |

The references come from ThorVG itself at the pinned tag. A bump regenerates
them (`docs/dependency-updates.md`) and the PR shows each changed PNG.

## Bumping

Run `scripts/vendor-thorvg.sh <tag>`, build, regenerate the golden frames
and look at the diff images, run the `sticker_playback` tests and a `/verify`
capture. The weekly deps watch (`native:thorvg`) opens an issue when ThorVG
tags a release.

## Risks

- ThorVG defines the global `operator new` and `operator delete` as `malloc`
  and `free` (`tvgInitializer.cpp`). Linked statically, they replace the C++
  allocator for the whole process, TDLib and ntgcalls included. Behavior is the
  same except that an allocation failure returns null instead of throwing,
  and every party treats that as fatal anyway.
- ThorVG aborts the process on allocation failure in its constructors
  (the C API header says so). The size caps above bound what a sticker can
  ask for.
- Each thread that decodes keeps a ThorVG memory pool until exit. Decodes run
  on the fixed GPUI background pool, so this is bounded by its size.
- Windows (MSVC) and Linux (GCC) builds are proven by CI, not locally. The
  `__STDCPP_THREADS__` check turns a silent race into a build error if a
  toolchain lacks it.
- Two of the golden fixtures are real stickers from Telegram packs, kept for
  real-world coverage; replace them with hand-made files if that is a concern.
