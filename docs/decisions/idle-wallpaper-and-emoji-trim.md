# Wallpaper pictures and emoji glyphs leave the GPU once idle

Status: done. Found in the 2026-10 memory sweep (see `sticker-frame-leak.md`
and `owned-images.md`), measured on an M1 Pro with a release demo-capture
build of main after #1233.

## Wallpaper pictures

`wallpaper_cache` renders one picture per wallpaper and area size, on a
worker, and kept up to 64 MB of them until the budget pushed old ones out.
Each window width makes a new full-history picture, so resizing the window
through six widths and idling 30 s went from 73 to 144 MB: the atlas held
seven pictures (4.4 to 7.9 MB each, each in a texture of its own) and the
same again in RAM, while only one was on screen. A turned gradient, a
wallpaper since changed, and a closed dialog's preview and thumbnails stayed
the same way.

Each picture now records where and when it was last painted
(`image_budget::paint_stamp`) and in which windows. At the idle trim,
`release_unshown` takes out the pictures no slot shows and no frame can
still show (a cached slice replaying an older frame), and drops them from
the atlas: old window sizes, a wallpaper since turned or changed, a closed
dialog's thumbnails, a closed chat window's history. A slot's current
picture always stays (the dialog preview's too, about 1 MB). Frames are
counted in the main window only, so a picture another open window painted
stays until that window closes. A new picture counts as activity, so the
idle trim also runs after a resize, which shows nothing else new.

## Emoji glyphs

GPUI rasterizes each color emoji once per size and keeps the tile in the
window's sprite atlas until the window closes. One visit to the emoji panel
rasterized 543 emoji: 16 MB of GPU memory (four 4 MB textures), kept for
good. Scrolling through the whole panel would keep about 60 MB on the GPU.

The atlas (vendored `gpui-pre`, `AtlasState`, shared by the Metal, wgpu and
DirectX backends) now records the use mark each tile was last asked for
under and counts its emoji glyphs. `Window` gains `emoji_glyph_count`,
`begin_emoji_glyph_sweep` and `sweep_emoji_glyphs`.

A sweep (`src/ui/glyph_sweep.rs`) runs in two steps, because a cached slice
replays glyphs from an earlier frame without asking the atlas for them:

1. From the idle trim, between frames: start a new use mark and refresh the
   window, so the next frame redraws every view, cached ones too, and asks
   for every emoji on screen under the new mark.
2. Once that frame is drawn (deferred from its `QuillApp::render`): drop the
   emoji glyphs nothing asked for under the mark.

The emoji on screen keep their tiles. A sweep costs one ordinary frame, and
an idle window needs no frame beyond it.

When it runs:
- At the idle trim, when the atlas holds more than 128 emoji and twice what
  the last sweep kept, so a screen full of emoji isn't redrawn after every
  scroll. New emoji glyphs count as activity for the idle trim.
- At the idle trim after the window went to the background, whatever the
  count: nothing tells how many emoji left the screen (a closed emoji
  panel), and a redraw behind another app costs nothing the user sees.

Not covered: macOS keeps its own decode cache for every emoji drawn (about
7 KB each in malloc, 3.9 MB after the panel visit). Drawing panel emoji from
Quill's own sprite images would release that too.

Live run (Linux, wgpu, 2x): with the reaction menu open, the idle sweep
dropped 0 and kept all 22 emoji. After the menu closed and the window lost
focus, it dropped 20 and kept 2, and the two reaction chips still drew.

## Tests

- `wallpaper_cache::tests::once_idle_only_what_a_slot_shows_stays`
- `wallpaper_cache::tests::a_picture_painted_last_frame_stays`
- `wallpaper_cache::tests::what_another_open_window_painted_stays_until_it_closes`
- `glyph_sweep::tests::a_sweep_drops_only_the_emoji_nothing_drew_since_it_began`
  (the atlas side, on GPUI's headless atlas)
- `glyph_sweep::tests::text_glyphs_stay_whatever_their_mark`
- `glyph_sweep::tests::a_screen_of_emoji_is_not_swept_again_until_it_doubles`
- `glyph_sweep::tests::going_to_the_background_sweeps_any_emoji`
