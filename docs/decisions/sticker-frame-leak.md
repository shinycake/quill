# Sticker frames a late view showed never left memory

Status: fixed. Found in the 2026-10 memory sweep: v2026.1029.3985 grew to
about 550 MB on a Mac after 40 minutes of normal use, while a fresh launch
idles at 91 MB and stays flat.

## Symptom

With an animated sticker on screen, memory grew for as long as the sticker
played and never came back, not even after leaving the chat. Measured on an
M1 Pro (120 Hz display) with a release demo-capture build of 0b5ee7b2c, one
run per case (footprint, MB):

| Case | Kept after 30 s idle |
| --- | --- |
| One 60 fps TGS sticker, 40 s on screen | +126 (GPU +100, Rust blocks +25) |
| TGS and WebM sticker | +173 |
| Animated demo history with the GIF panel | +700 |
| Three animated custom emoji | +2 |

The worst run reached 1237 MB and stayed at 1196 MB after 90 s of idle.

## Cause

The streaming player (`src/ui/sticker_player`) shows an earlier ready frame
when the frame a view's play head reached is not drawn yet. It recorded that
earlier frame under the index the head reached, not its own. `Player::ask`
then found the frame's real index stale, took it out of `ready` and retired
it, because the recorded index didn't match. The frame was still shown, and
when the next frame replaced it, `frame` retired it a second time.

`image_budget::sweep` drops a retired image only when it holds the last
reference (`Arc::strong_count == 1`). Two entries for one image each saw a
count of 2, so neither was ever dropped: the pixels stayed in the
large-block arena and the tile stayed in the GPU atlas, where a few live
tiles also keep whole atlas textures alive.

A late frame is the norm on a 120 Hz display (the finer frame grid asks for
frames the workers have not drawn yet), which is why the Mac leaked fastest.
At 1x on Linux the frames were ready in time; at 2x
(`GPUI_X11_SCALE_FACTOR=2`) the Linux demo leaked too: 960 retired images
(240 MB) after 40 s, 954 of them held only by a second retired entry.

## Fix

- `Player::frame` records the index of the frame it actually shows. `ask`
  still drops that frame from `ready` once no head wants it, but no longer
  retires it while it is on screen; it is retired once, when it stops
  showing.
- `image_budget` keeps retired images by image id. Retiring an image that is
  already waiting only moves its frame to the later one, which keeps it at
  least as long as either retirement would. Any other double retirement can
  no longer pin an image. One was there too: `trim` and `stop` retire both
  `ready` and `shown`, which hold the same frame most of the time, so every
  clip that closed (a chat switch, 5 s off screen) lost one frame for good.
  The id key fixes that case on its own.

## Measured after the fix

Same Mac, release demo-capture builds of 0b5ee7b2c (base) and the fix,
alternating, a fresh launch per run. Kept is footprint after the idle minus
launch idle.

| Run | Base kept | Fix kept |
| --- | --- | --- |
| One 60 fps sticker 40 s, 30 s idle, round 1 | +161 MB | +2.8 MB |
| Same, round 2 | +81 MB | +8.4 MB |
| Animated history, GIF and sticker tabs 60 s, 90 s idle, round 1 | +1086 MB (peak 1220) | +0.9 MB (peak 139) |
| Same, round 2 | +1092 MB (peak 1228) | +4.4 MB (peak 145) |

Late frames were as common with the fix as without (about 41% of shown
frames in the animated history), so the fix runs took the same path. In
base, kept memory followed the late count, about 1.25 MB per late frame for
the single sticker. The sticker still animates in captures of the fix build.

## Tests

- `image_budget::tests::an_image_retired_twice_still_leaves`
- `sticker_player::tests::every_frame_a_late_head_showed_leaves_once_the_clip_stops`

Both check that every frame comes back from the sweep (so it leaves the
atlas) and that nothing holds it after. Both fail with the old code and pass
now.
