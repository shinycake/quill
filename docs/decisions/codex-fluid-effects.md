# Fluid effects: Thanos delete, spoiler "mess", round video seek

Three tdesktop effects, rebuilt from its sources and checked side by side
with Telegram Desktop.

## What GPUI offers

- **No pixel readback.** `Window::render_to_image` only exists behind
  `test-support`, which also turns on GPUI's leak detection, so it can't
  ship. Effects can't snapshot what was drawn.
- **Quads are cheap.** `paint_quad` batches into instanced draws: a few
  thousand per frame at 60 fps is fine.
- **Multi-frame images.** A `RenderImage` holds many frames and
  `paint_image` picks one: pre-rendered animation tiles cost one draw
  each.
- **Clipping and paths.** `with_content_mask` clips a subtree to any
  rectangle (`CutLeft`); `PathBuilder::stroke` draws arcs.
- **Frame clock.** Visible history rows are rebuilt every frame while
  something asks the frame clock for ticks; anything painted at paint
  time just reads the clock.

## Thanos delete (`ui/vanish.rs`, tdesktop `Ui::ThanosEffect`)

tdesktop's version is a GPU particle system (shaders `thanos_*.comp`):
- the message is snapshotted, one particle per pixel;
- particles are released by a window sweeping left to right over the
  first ~0.5 s;
- each flies off at 32–64 px/s plus a constant drift and fades over ~2 s;
- the gap collapses in 400 ms plus 0.15 ms/px, at most 600 ms, half-sine.

Without a snapshot, Quill builds the dust from what the message is made
of:
- a grid of square grains (1.5 pt, coarser past ~7000) over the
  bubble's painted bounds, skipping the rounded corners;
- grains take the bubble's fill with slight shade jitter, about one in
  six inside the padding takes the text color, and a photo's grains
  sample its minithumbnail;
- the real message stays on screen, cut away at the crumbling front, so
  the words are there until their dust takes over.

Grains ease into motion (the integral of a smoothstep), rise, hold
0.35–1.25 s and fade in 0.5 s. When the front has crossed, the row
becomes an empty gap that closes with tdesktop's half-sine. Row and
bubble bounds come from invisible trackers painted with each row. The
dust paints in an overlay over the conversation, so it outlives the row.

## Spoilers (`ui/spoiler_fx.rs`, lib_ui `spoiler_mess.cpp`)

lib_ui's spoiler "mess" is a 128 pt tile of specks:
- 3000 for media, 9000 for text;
- each lives 600 ms (fade in, shown, fade out) and drifts 10–20 or
  4–8 device px/s;
- starts are spread over a 60-frame, 33 ms loop;
- sprites are 1.5–2 pt rounded rects stamped on whole device pixels.

Quill generates the same field from a fixed seed.

- **Media:** the 60 frames are rendered once into one multi-frame
  image, tiled over the blurred preview, which is darkened by
  `kImageSpoilerDarkenAlpha`. The first try drew soft sub-pixel specks
  and looked half as dense as Telegram's; stamping on whole pixels and
  using device-pixel speeds matched it. A reveal fades the cover out
  over the media in 200 ms (`fadeWrapDuration`).
- **Text:** hidden runs fade their glyphs out (`fade_out: 1`; a clear
  highlight color is blended, so it changes nothing). Specks in the
  text's color fill the run's line rectangles from the text layout.
  Revealed specks fade out over the text. The opaque gray blocks are
  gone, in both the plain and the custom-emoji paragraph paths.

## Round video seek (`ui/round_seek.rs`, tdesktop `VideoMessageSeek`)

A round video message playing with sound shows a 3 px progress arc
(72%), clockwise from the top. A click pauses it, and the seek ring
springs in:
- 220 ms ease-out-back in, 150 ms out;
- a radial edge shade (a cached gradient image, since concentric
  strokes banded);
- a 20% track, the line growing to 5 px with an 18 px inset;
- a 16 px dot that grows to 22 px while dragged.

Pressing within the outer 40/240 of the radius and dragging seeks; the
clip holds still while dragged and resumes if it was playing. Another
click resumes. When playback ends it returns to the muted loop.

Verified live in Saved Messages:
- spoiler density and brightness side by side with Telegram Desktop;
- the reveal fade;
- a text spoiler and its reveal;
- two deletes of my own test texts, frame by frame;
- the ring pausing, springing in, and seeking by drag.
