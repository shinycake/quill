# Decode thread panic ends in the player's error state (sweep S-06)

## What tdesktop does
Media playback failures surface as a failed state on the viewer; there is no
"loading forever". Nothing in tdesktop maps to Rust unwinding.

## Finding
`Player::spawn` already ran the decode thread under `spawn_guarded`, so a
panic did not kill the process. But the guard only swallowed it: `error` and
`eof` were set solely by `decode_loop` on a returned error, so a panic left
the player without an error and the UI on "loading" forever.

## Change
- `src/video_decode/mod.rs`: an `ExitReport` drop guard lives on the decode
  thread. When the thread ends (panic unwind or unexpected return) without a
  stop request, it records an error if none exists, sets `eof`, clears the
  picture queue and releases the sound end. The UI already shows
  `Player::error()` through the ffmpeg backend, with its existing retry.
- The `Player` keeps the thread handle (detached on drop, so the UI never
  blocks); tests join it.
- Other long-lived media workers (call feeder, screen capture, TDLib
  receive, voice encoder, round preview) all already use `spawn_guarded`;
  the live-stream backend runs on GPUI tasks, not threads. No unguarded
  worker found.

## Verification
- `decode_panic::a_decode_panic_puts_the_player_in_its_error_state`: a
  demuxer panics on its 4th call; the player reaches the error state, queue
  and sound are released, the thread joins without hanging.
- `decode_panic::dropping_a_player_mid_decode_neither_errors_nor_hangs`:
  a normal drop sets no error.
- Before the change the first test times out (error stays `None`).
