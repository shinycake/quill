# Playback at 1x does no stretcher search

## What was wrong

Measured on the owner's Mac (M1 Pro, offline demo, release build of main),
any audio playback at 1x cost about a quarter of a core:

| File | CPU at 1x |
| --- | --- |
| 48 kHz WAV | 23.2 to 23.7% |
| 44.1 kHz WAV | 24.8% |
| 48 kHz Opus | 25.7% |

Voice notes, audio files and, on Linux and Windows, video soundtracks
(`StreamSound`) all play through the speed stretcher (`src/ui/audio/tempo.rs`).
A profile put the time in `Tempo::pick`. For every 20 ms block it scored
every even position in a +-12 ms window (577 positions at 48 kHz, each a
480-term correlation), even when the natural continuation (the
previous slice's start plus one hop) was in that window. By the routine's own
comment, nothing else can win there. At 1x that is every block, so 1x paid
for a search whose answer was known.

## What changed

`Tempo::pick` returns the natural continuation without scoring anything when
it lies in the window. Below about 83.7 kHz that is provably the result the
old search reached:

- The natural continuation counted as a perfect score of 1, and another
  position had to score more than 1 + 1e-4 to take over.
- A normalised correlation is at most 1 (Cauchy-Schwarz). f32 rounding of
  its sums of k = rate/100 terms adds at most about 2ku (u = 2^-24): 5.7e-5
  at 48 kHz and 5.3e-5 at 44.1 kHz, below the 1e-4 margin. That worst case
  stays below the margin up to about 83.7 kHz, which covers every voice note
  (Opus decodes at 48 kHz) and nearly all music.

Above 83.7 kHz the result can differ on contrived input. Review built one, a
full-scale spike followed by a constant tail near -72 dBFS, that gains about
1.41ku from rounding: 8.1e-5 at 96 kHz (the natural continuation still
wins), but 1.5e-4 and 1.6e-4 at 176.4 and 192 kHz, where main would have
picked another position. That is a one-off shift of at most 12 ms to a slice
that correlates almost exactly with the natural one, with no audible seam.
Skipping the search keeps the plain continuation, which is the more
transparent output. Pure tones, the closest ordinary audio comes to a tie,
gain only about 1e-6 from rounding.

At 1x, the offset between the natural continuation and the nominal position
stays the same from block to block, so the natural continuation never leaves
the window and 1x scores nothing. After a switch back to 1x from another
speed, it can be outside the window for one block. That block searches as
before; the blocks after it do not.

Other speeds still search whenever the stretch has pulled the natural
continuation out of the window, and skip the search when it has not. Their
output is unchanged (with the same sample-rate bound), and they also do less
work. Positions scored per 20 ms
block on a 1.6 s, 48 kHz test sound:

| Speed | Main | Now |
| --- | --- | --- |
| 0.5x | 569 | 383 |
| 0.7x | 567 | 318 |
| 0.75x | 566 | 310 |
| 1x | 562 | 0 |
| 1.2x | 560 | 416 |
| 1.5x | 556 | 493 |
| 2x | 549 | 535 |

### What stays the same

- **Read-ahead.** `pick` still reads input up to where a search could look.
  What has been read is the player's position (`engine::Counted` sits below
  the stretcher) and a video's audio clock (the decoder keeps time from what
  is pulled, and `AUDIO_OUTPUT_LATENCY` includes the read-ahead). So pause
  points, the restart spot after a stop or a paused seek, and lip-sync are
  unchanged. The local experiment that also skipped the read-ahead changed
  one sample at a pause point and the restart spot after it.
- **Overlap-add and its state.** The accumulator, previous slice and
  nominal position update exactly as before, so a 1x to 1.5x to 1x switch
  crossfades exactly as main did.

### Alternative rejected: a true passthrough at 1.0

A passthrough that bypasses the overlap-add at exactly 1.0 was rejected for
two reasons:

- The two windowed halves sum to the input only within f32 rounding, so
  bypassing them would change the output bits.
- The WSOLA state would have to be rebuilt at every speed switch.

## Tests

Under test, `Tempo::pick` still runs the search on every block it skips and
asserts that the search picks the same position. So every test that plays
through the stretcher checks the change on every block, including the
existing ones in `audio::tests` and the engine's.

During development, a verbatim copy of main's routine also confirmed
bit-identical output, and the same input read after every sample, in all the
scenarios below. Review replaced that copy with the per-block check, which
keeps checking as the stretcher changes.

`src/ui/audio/tempo_tests.rs` drives the check with a sound built to come
close to a tie. It has four parts: a tone that repeats exactly every whole,
even number of frames (positions one period away score within rounding of
the natural one), silence, a 100 Hz to 4 kHz sweep, and noise. Each channel
is different.

| Test | What it checks |
| --- | --- |
| `at_1x_every_block_is_what_the_search_picks` | 1x at 48 kHz stereo, 44.1 kHz mono, 16 kHz mono and 96 kHz stereo, through a seek back, a seek forward and the end. |
| `at_other_speeds_every_block_is_what_the_search_picks` | 0.5x, 0.7x, 0.75x, 1.2x, 1.5x and 2x with a seek, at 16 kHz mono and 48 kHz stereo. The length matches the speed. |
| `across_speed_switches_every_block_is_what_the_search_picks` | 1x, another speed, 1x, the other speed again, a seek and 1x, switched mid-block, for 1.5x, 0.75x, 2x and 0.5x. |
| `at_1x_the_stretcher_reads_ahead_as_far_as_a_search_would` | At 48 kHz stereo, 8,832 samples are read before the second block, as on main, and before every later block at least as far as a search could look. The same holds again after a seek. |
| `at_1x_the_stretcher_never_searches` | 1x at 48 kHz stereo with a seek scores 0 positions. |
| `back_at_1x_the_search_stops_within_a_block` | After 1.5x, 0.75x, 2x or 0.5x, at most one block (577 positions) searches back at 1x. |

With main's search back in the window (and the read-ahead skipped there, as
in the experiment), three tests fail. `at_1x_the_stretcher_never_searches`
counts 38,659 positions in 69 blocks at 1x.
`back_at_1x_the_search_stops_within_a_block` fails too. The
read-ahead test reads 5,888 samples before the second block instead of
8,832, about 31 ms less: that is the position a pause leaves and the restart
spot after it.

With the margin flipped (-1e-4 instead of +1e-4), the per-block check fails
in every test: the search moves the slice a whole number of tone periods
(from frame 960 to 1440 at 48 kHz).

## What to measure on the Mac

Use a release build of this branch, the offline demo and the same files as
before. Read Quill's CPU (`top -pid`, or Activity Monitor) while each file
plays at 1x:

| File | Before | Expected now |
| --- | --- | --- |
| 48 kHz WAV | 23.2 to 23.7% | about 5 to 6% |
| 44.1 kHz WAV | 24.8% | about 5 to 6% |
| 48 kHz Opus | 25.7% | about 12 to 21% |

Most of the Opus figure is decoding (see below). The expected figures are the
local experiment's. That experiment also skipped the read-ahead. Keeping it
moves the same samples, only earlier, so the cost should be the same.

Also check the following:

- **Recorded output.** Record 1x output with the same capture as the
  experiment and compare it with main's. Expect it to be identical,
  including at the pause point and the restart spot after it.
- **Speed switches.** Switch 1x to 1.5x to 1x on a playing voice note.
  Expect no click or jump (the samples are the same as main's).
- **Other speeds.** Play a voice note at 1.5x and 0.75x. Expect the same
  sound and slightly lower CPU than main.

## Not fixed: Opus decoding

Most of the remaining Opus playback cost was in the `opus-decoder` crate
(0.1.1) recomputing sin and cos in its CELT inverse transform.
`MdctBackward::backward` (`src/celt/mdct.rs:213` in the crate) calls
`flat_fft_forward` (`src/celt/kiss_fft.rs:116`), a direct O(n²) DFT that
calls `sin_cos` in f64 for every term (line 123). That is about 230,000 calls
per 20 ms frame per channel for the long transform. This was measured on a
64 kbps test tone; real voice notes may differ.

A fix would precompute the twiddles (the crate's `KissFft` already does for
its own `ifft`) or use a real FFT. That means vendoring or patching the
crate, so this change does not include it.
