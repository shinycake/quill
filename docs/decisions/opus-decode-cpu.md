# Opus voice notes decode with an FFT

## What was wrong

After the stretcher fix (`docs/decisions/tempo-1x.md`), a 48 kHz Opus file
still cost 16.3 to 19.8% CPU at 1x on the owner's Mac (M1 Pro, release
build), against 5.5% for the same audio as WAV. A profile put the
difference in opus-decoder 0.1.1, the pure-Rust decoder behind
`src/ui/audio/opus.rs`.

CELT (Opus's transform codec, used for music and for the upper band of
hybrid speech) rebuilds each frame with an inverse MDCT, whose core is an
N/4-point complex FFT: 480 points for a 20 ms frame. opus-decoder computed
it as a direct DFT (`flat_fft_forward` in `src/celt/kiss_fft.rs`) that
called `f64::sin_cos` for each of its 480 x 480 terms: 230,400 calls per
20 ms frame and channel, 11.5 million a second for mono. Under callgrind,
`MdctBackward::backward` (with that DFT inlined) is 98.8% of the
instructions spent decoding a 64 kbit/s mono stream, and `sincos` alone
82.6%. The MDCT even held a `KissFft` plan with precomputed twiddles, but
did not use it.

crates.io has no release after 0.1.1 (checked 2026-10-31). The upstream
repository's master branch has already replaced the DFT with a mixed-radix
FFT, unreleased.

Running the same decode under strace showed one more leftover: on every
CELT frame, and again on frames with the pitch postfilter on, the decoder tried
to open a debug log at `/Users/tadeusz/Opus/Rasopus/.cursor/debug-bea564.log`
(408 failed `openat` calls for 5 s of audio).

## What changed

opus-decoder 0.1.1 is vendored in `third_party/opus-decoder` and patched
through `[patch.crates-io]` (`third_party/opus-decoder/QUILL-CHANGES.md`
lists the changes):

- `kiss_fft.rs` and `mdct.rs` are upstream master's versions: a radix
  4/2/3/5 FFT over precomputed twiddles (covering the 60, 120, 240 and
  480 points that 2.5 to 20 ms frames need), run with buffers the MDCT keeps.
  Taking upstream's code rather than writing a new FFT means the patch can
  go when a release has it.
- The two debug-log appends are removed (upstream removed them too).
- Other leftover debug work no longer runs while decoding: two `format!`
  strings built for no-op loggers (on intra-energy and stereo frames) and
  five process-wide `SeqCst` counters. The output is bit-identical, and the
  instruction count moves by under 0.2%: the release build had already
  optimized the strings away.

Upstream's other changes since 0.1.1 are not taken: they save little next
to the FFT, and two of them change output bits.

The crates.io package has no license file. `LICENSE-MIT` and
`LICENSE-APACHE` come from the upstream repository at the 0.1.1 release
commit, and `THIRD_PARTY_LICENSES.md` is regenerated, so it now has the
crate's own MIT copyright line instead of the generic MIT text.

## Is the output the same?

Not bit for bit; it is closer to libopus than before. The DFT's twiddles
came from an `f32` pi widened to `f64`, so at 480 points their phase was up
to 8.4e-5 rad off, and its results were up to 4.7e-5 (relative to the
largest bin) from the exact transform. The FFT's are at most 3.6e-7 off.
opus-decoder rounds its output to 16 bits, so most samples come out
identical and the rest move by one step.

Decoded with a harness before and after the change (all samples kept):

| Stream (encoded by libopus) | Mode | Samples that changed | Largest change | SNR to libopus before / after |
| --- | --- | --- | --- | --- |
| speech, mono, 64 kbit/s, 20 ms | CELT | 2.28% | 1 step | 90.3 / 108.4 dB |
| speech, mono, 32 kbit/s VoIP, 20 ms | hybrid | 0.0016% | 1 step | 118.3 / 119.1 dB |
| speech + noise, mono, 32 kbit/s VoIP, 60 ms | hybrid | 0.054% | 1 step | 106.0 / 111.5 dB |
| speech, mono, 16 kbit/s VoIP, 20 ms | mostly SILK | none | 0 | 53.5 / 53.5 dB |
| speech, mono, 24 kbit/s, 40 ms | mostly SILK | none | 0 | exact / exact |
| music, stereo, 128 kbit/s, 20 ms | CELT | 5.32% | 1 step | 85.8 / 106.3 dB |
| music, stereo, 64 kbit/s, 10 ms | CELT | 2.83% | 1 step | 88.5 / 106.9 dB |
| music, stereo, 96 kbit/s, 5 ms | CELT | 1.31% | 1 step | 90.3 / 93.6 dB |
| music, mono, 48 kbit/s, 2.5 ms | CELT | 0.57% | 1 step | 95.5 / 107.7 dB |
| music, stereo, 32 kbit/s, 60 ms | mostly CELT | 5.25% | 1 step | 85.8 / 106.2 dB |
| music, mono, 64 kbit/s low-delay, 20 ms | CELT | 4.99% | 1 step | 86.0 / 106.5 dB |
| music, stereo, 24 kbit/s VoIP, 20 ms | CELT + hybrid | 5.02% | 1 step | 85.6 / 105.5 dB |
| RFC 8251 vectors 01 to 12 | all | 0 to 3.7% | 0 or 1 step | equal or better on each |

One step is 1/32768 of full scale (-90 dBFS), the 16-bit output's own
rounding step, so the change is inaudible: the difference sits 82 to
122 dB below the signal. Against libopus (ffmpeg's libopus decoder for the
streams, the vectors' `.dec` files for the vectors), every stream that uses
CELT is now as close or closer, by up to 20.5 dB. SILK frames do not use
the MDCT and are unchanged. The crate's RFC 8251 conformance test passes
all 12 vectors.

The 53.5 dB on the 16 kbit/s SILK stream is an existing difference in
opus-decoder's SILK path (up to 100 steps from libopus), not part of this
change. Neither is a smaller one in some stereo transient frames: a stereo
96 kbit/s 5 ms stream and a stereo 64 kbit/s 10 ms stream with noise bursts
come out 7.2 and 8.3 steps from rusty-opus, the same before and after (the
mono streams and a stereo 128 kbit/s 5 ms one stay under one step).

## How fast

CPU time to decode, in ms per second of audio, on this Linux machine
(release profile; best of five runs, interleaved):

| Stream | Before | After | Ratio |
| --- | --- | --- | --- |
| speech, mono, 64 kbit/s, 20 ms (CELT) | 173.9 | 3.14 | 55x |
| speech, mono, 32 kbit/s VoIP, 20 ms (hybrid) | 156.6 | 2.60 | 60x |
| music, stereo, 128 kbit/s, 20 ms (CELT) | 427.5 | 6.29 | 68x |
| music, mono, 48 kbit/s, 2.5 ms (CELT) | 27.2 | 3.09 | 8.8x |
| speech, mono, 16 kbit/s VoIP, 20 ms (mostly SILK) | 2.52 | 1.14 | 2.2x |

Instructions counted by callgrind for 5 s of audio agree: 7.77 G to
145 M (54x) for the 64 kbit/s speech, 18.8 G to 286 M (66x) for the stereo
music and 7.15 G to 157 M (46x) for the hybrid speech. The SILK stream gains from its
few hybrid packets. Short frames gain less because the DFT cost grows with
the square of the frame length. After the change the FFT is about a quarter
of what is left, behind band decoding.

The MDCT keeps three buffers per transform size: about 22 KB per decoder,
one decoder per playing voice note.

## Tests

- `ui::audio::opus::tests::celt_streams_decode_within_one_lsb_of_the_reference_decoder`
  (`src/ui/audio/opus_tests.rs`, runs in CI): voice-like mono and
  music-like mono and stereo, encoded CELT-only by rusty-opus at 24 to
  128 kbit/s in 2.5, 5, 10, 20 and 60 ms packets, decoded by opus-decoder
  and by rusty-opus's decoder (a libopus port that matches the RFC 8251
  vectors' reference output to within one step). The music's noise bursts
  make the encoder code 4 to 8 frames of each 20, 10 and 5 ms music stream
  as transients (short blocks, so 60-point FFTs). Every sample must be
  within one 16-bit step: rounding accounts for half a step, and this branch
  is at most 0.62 steps off. opus-decoder 0.1.1 fails it (1.05 steps on the
  24 kbit/s voice).
- `celt::kiss_fft::tests::fft_matches_the_direct_dft_it_replaced_for_every_celt_size`
  in the vendored crate, which CI runs with the module's other tests through
  `scripts/test-opus-fft.sh` (`rustc --test` on the std-only file): for 60,
  120, 240 and 480 points, the FFT is within 1e-6 of an exact `f64` DFT
  (measured 1.7e-7 to 3.6e-7) and within 1e-4 of 0.1.1's DFT, kept verbatim
  in the test. The 0.1.1 DFT fails the first bound (5.2e-6 at 60 points).

## What to measure on the Mac

Use a release build of this branch, the offline demo and the same 48 kHz
Opus file as before. Read Quill's CPU (`top -pid`, or Activity Monitor)
while it plays at 1x:

| File | Before | Expected now |
| --- | --- | --- |
| 48 kHz Opus | 16.3 to 19.8% | about 5.5 to 7%, near the WAV figure |
| 48 kHz WAV | 5.5% | unchanged |

Decoding should drop from 11 to 14% of a core (the gap to WAV) to under
0.5%.
