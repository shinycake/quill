# Changes made by the Quill project

This directory is opus-decoder 0.1.1 from crates.io (MIT OR Apache-2.0), the
Ogg/Opus voice-note decoder, used through `[patch.crates-io]` in Quill's
`Cargo.toml`. Commit `fb0fed0f` added the unmodified registry copy (without
`.cargo-ok`, `.cargo_vcs_info.json`, `Cargo.lock` and `Cargo.toml.orig`), so
`git diff fb0fed0f -- third_party/opus-decoder` shows every change. Changed
code is marked `QUILL`. Background and measurements:
`docs/decisions/opus-decode-cpu.md`.

The crates.io package ships no license file, although its README links two.
`LICENSE-MIT` and `LICENSE-APACHE` are the upstream repository's files at the
0.1.1 release commit (`82fe31d5`, github.com/TadeuszWolfGang/Rusopus), so
`THIRD_PARTY_LICENSES.md` carries the crate's own copyright line.

## Changes

`src/celt/kiss_fft.rs`, `src/celt/mdct.rs`: the CELT inverse MDCT computed
its N/4-point complex transform (480 points for a 20 ms frame) as a direct
DFT, `flat_fft_forward`, which called `f64::sin_cos` for each of its
N/4 x N/4 terms: 230,400 calls per 20 ms frame and channel, about 98% of
the instructions spent decoding a 64 kbit/s stream. Both files are now
upstream's unreleased versions from its master branch (plus the `QUILL`
markers and test): a mixed-radix
(4, 2, 3, 5) FFT over precomputed twiddles, run with buffers the MDCT keeps
(`MdctBackward::backward` takes `&mut self`). CELT and hybrid streams decode
46 to 68 times faster. The FFT is closer to the exact transform than the
DFT was (the DFT's twiddles carried the error of an `f32` pi), and the
16-bit output moves by at most one step. The test at the end of
`kiss_fft.rs` (`fft_matches_the_direct_dft_it_replaced_for_every_celt_size`)
keeps the old DFT as its reference.

`src/celt/mod.rs`: the four MDCTs are passed as `&mut` for the above, and
two leftover debug-log appends to `/Users/tadeusz/Opus/Rasopus/.cursor/...`
are removed (upstream master removes them too). One opened that file on
every CELT frame and one on every frame with the pitch postfilter on; on
any other machine both failed with ENOENT, so the output is unchanged.

`src/celt/mod.rs`, `src/celt/quant_bands.rs`, `src/lib.rs`: leftover debug
work on the decode path is gone, with output bit-identical on 24 test
streams. Two `format!` strings built for no-op loggers, on every
intra-energy frame and every stereo frame, are gated behind the
constant-false `debug_packet` like the others. Five process-wide `SeqCst`
counters bumped per packet or frame are the constant `usize::MAX`
(upstream master does the same); their values only fed disabled traces,
some keyed to particular packet numbers, and `usize::MAX` matches none.

`src/celt/kiss_fft.rs`: the module doc said the forward transform applies
`1/N`; both directions are unscaled.

Master's other changes (fewer allocations, `exp2` for `powf`, a polynomial
sine in the spreading rotation) are left out: they save little next to the
FFT, and the last two change output bits. When a release after 0.1.1 has the
FFT, depend on it and delete this directory.

## Running the crate's tests

`third_party/opus-decoder` is not a workspace member. CI runs the FFT's
tests (`src/celt/kiss_fft.rs`, std-only) through `scripts/test-opus-fft.sh`,
which builds that file alone with `rustc --test`. To run the rest, add
it to `[workspace] members` and delete its `criterion` dev-dependency (used
only by the benches the crates.io package leaves out), run
`cargo test --release -p opus-decoder`, then revert both edits. The RFC 8251
vectors (opus-codec.org/static/testvectors/opus_testvectors-rfc8251.tar.gz)
run with `OPUS_TESTVECTORS_DIR=<vectors dir> cargo test --release
-p opus-decoder --tests -- --ignored`, where `<vectors dir>/vectors.txt` has
one line per vector, such as
`testvector01 testvector01.bit testvector01.dec 48000 2`.
