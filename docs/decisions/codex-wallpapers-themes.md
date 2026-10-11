# Wallpapers: one renderer for every background type, Blur, Tile, preview, gradient bubbles

Earlier slices (`codex-appearance-wallpapers.md`, `codex-chat-wallpapers-themes.md`) wired the TDLib calls, the installed list, per-chat wallpapers and emoji themes. They also left gaps: freeform gradients painted as two stops, the pattern was a normal-mode black overlay, Blur and Tile were missing, a gallery click applied the wallpaper with no preview, and a `bg/` link lost its mode and intensity on Apply.

## What tdesktop does

- `ui/chat/chat_theme.cpp`:
  - `CacheBackground` renders the whole background once per area size, off the main thread, and paints the cached image. After a resize it stretches the old image until the new one is ready (`kCacheBackgroundFastTimeout`, 200 ms).
  - Fills come from lib_ui `GenerateGradient`. Two colours make a linear gradient, corner to corner on the diagonals for rotations of 45. Three or four colours make the 64 x 64 `GenerateSmallComplexGradient` picture, smooth-scaled.
  - A pattern is scaled to the area height, keeping its aspect, and tiled in an odd number of centred columns. Positive intensity uses Qt `SoftLight` at that opacity, black over light fills and white over dark ones (`IsPatternInverted`, HSV value at most 0.3). A negative intensity uses `DestinationIn` plus black at `1 + intensity`.
  - Blur (`PrepareBlurredBackground`) fits the photo in 900 px and stack-blurs it with radius 24. A covering photo is cut out by `ComputeChatBackgroundRects`. Tile repeats the photo at one image pixel per device pixel.
  - Dark-mode dimming applies to photos only.
- `settings/sections/settings_chat.cpp` has "Choose from gallery", "Choose from file" and "Tile background". Tile is a local setting per day and night, shown only for photo wallpapers. A file opens the preview box.
- `boxes/background_box.cpp`: the gallery. The corner × shows only on wallpapers that are not in use and not a default, and asks "sure delete" before removing.
- `boxes/background_preview_box.cpp`: the preview has Blur for photos, Share (copies the link) for account wallpapers, and Apply. Desktop has no Motion option and never animates `is_moving`.
- `ChatTheme::adjustPalette` and `PrepareBubblesBackground`: one outgoing colour fills the bubbles. Several colours make a vertical gradient laid over the message area, and each bubble shows the slice it covers.
- TDLib `LinkManager` keeps `mode`, `intensity`, `bg_color` and `rotation` in `internalLinkTypeBackground.background_name`. `searchBackground` applies them to the returned type (`apply_parameters_from_link`). `setDefaultBackground` with `type: null` uses the background's stored type, so those parameters are lost unless the type is sent back.

## What changed

- `src/wallpaper_raster/` (core, tested on Linux CI) has the pixel maths, written as pure functions over BGRA buffers:
  - fills: solid, the lib_ui linear gradient, and freeform through `profile_photo_gradient::small_complex_gradient` (now public, with a phase argument; wallpapers use phase 0, the emoji avatar phase 1);
  - Qt's `soft_light_op` and constant-alpha interpolation, folded into a 256 x 256 table per composition;
  - the inverted mix;
  - odd centred pattern columns, the cover rectangles, the 40:1 trim, triangle blur (two box passes, the weights of lib_ui's stack blur), tiling.
- `ui/wallpaper_source.rs` runs on the wallpaper thread. It decodes photos with `image`, rasterises PNG and TGV patterns with resvg at the exact tile size, and keeps the last two of each.
- `ui/wallpaper_cache.rs`:
  - One worker thread. Each area is a slot: a new size replaces the slot's queued request and waits 150 ms for the size to settle, so a window drag renders once.
  - Results become one `RenderImage` per (wallpaper, pixel size) under a 64 MB LRU budget. Evicted images go through `image_budget::retire_all`.
  - While a new size renders, the area's previous image paints stretched.
  - If the thread cannot start, images render inline, so they still arrive.
- `ui/wallpaper.rs`: the `Wallpaper` enum is now Solid, Fill (two-stop or freeform), Image (blurred, tiled) and Pattern. Solid stays a plain fill; everything else is one canvas that paints one image.
- `ui/wallpaper_preview.rs` is a new dialog for gallery picks, `bg/` links and "From file…". It shows the wallpaper behind two sample messages and offers Blur for photos, Share (`getBackgroundUrl`, then the clipboard) and Apply. Apply sends `setDefaultBackground` with the chosen type, which keeps the link's mode and intensity and the Blur choice, or `inputBackgroundLocal` with `is_blurred`. The `bg/` preview moved here from `chat_look_ui.rs`.
- `ui/appearance/telegram_wallpapers.rs`: a gallery click opens the preview. The × shows only on wallpapers that are not in use and not a default, and asks before removing. "Tile background" appears for photo wallpapers (prefs `wallpaper_tile_day` and `wallpaper_tile_night`).
- `ui/bubble_theme.rs`: an emoji theme with several outgoing colours paints each outgoing bubble with its slice of the vertical gradient over the message area. The wallpaper layer reports the area on each frame. The text colour follows the fill's mean.

## Measurements (M1 Pro, release, 2400 x 1800 device px = a 1200 x 900 area at 2x)

| Work (on the wallpaper thread) | Time |
|---|---|
| Freeform fill | 34 ms |
| Linear fill | 30 ms |
| Soft-light table | 0.06 ms |
| Pattern composite (876 x 1800 tile) | 41 ms |
| Blur, 900 px photo | 6.5 ms |
| Tile | 2 ms |

Before, a pattern was a 512 px tile, stretched to the area height (soft at large heights) and painted as 3 to 5 image quads per frame. Its cache held up to six 1 MiB tiles plus six 256 KiB masks, about 7.5 MB. The tile lookup cloned a `PathBuf` every frame.

Now every non-solid background paints as one image quad per frame, with no pixel work on the UI thread. The trade-off is memory: one full-size image per visible area, 17.3 MB at 1200 x 900 @2x, the same approach tdesktop takes. The budget keeps the total under 64 MB. A resize re-renders once after the size settles; in between, the old image stretches.

## Honest choices

- Motion is not drawn. tdesktop has none on desktop, so `is_moving` is only kept and sent back.
- Bubble gradients are two-stop per bubble: from the area's colour at the bubble's top edge to the colour at its bottom edge. A middle stop inside one bubble is linearised. GPUI gradients have two stops.
- No dithering (tdesktop dithers gradient fills).
- The freeform rotation on send (`rotateComplexGradientBackground`, with a 200 ms crossfade) is not done.
- Photo wallpapers are decoded from the full file. tdesktop draws gallery thumbnails from the small thumbnail.

## Not done (follow-ups)

- `parity:updates-theme-colors`: the theme's `accent_color` does not yet recolour the chat (links, reply bars, service colours). tdesktop colourises the whole chat palette with it. Neither does `outgoing_message_accent_color` (links and meta inside outgoing bubbles), and there is no contrast fallback to black or white text. Media bubbles, polls and stickers still use the app accent rather than the theme fill.
- `parity:deeplink-bg-theme`: `bg/` links are complete. `addtheme/` links cannot be: TDLib says `internalLinkTypeTheme` has no theme support, and tdesktop uses MTProto `account.getTheme` directly.
- The chat colours dialog has no dark-mode dimming slider (`setChatBackground.dark_theme_dimming` is sent as 0) and no per-chat wallpaper from a file.
- Built-in themes and the theme editor (`parity:appearance-themes`) were not touched.

## Verified

- Core tests: every fill type; soft light against Qt's formula (black and white source, opacity); the inverted mix; pattern tile size and column centring; cover rectangles; the long-image trim; blur; tiling; `BackgroundType` serialisation round-tripped through the parser; the `getBackgroundUrl` and `setDefaultBackground` shapes; reducer handling of `httpUrl` and errors.
- UI tests: SVG, TGV and PNG masks at the tile size; the pattern composite; photo cover, tile, blur and a missing file; the slot queue (replacement, settling, dropping superseded renders); cache keys; Blur offered and sent; the remove rules; bubble slices; theme fill colours.
- Captures (temporary fixture, not committed), all viewed:
  - a freeform-gradient pattern in light mode;
  - an inverted pattern in dark mode;
  - a blurred photo and a tiled photo;
  - a two-colour gradient at 45 degrees;
  - the preview dialog with Blur;
  - the gallery with thumbnails, the × rules and Tile;
  - a three-colour theme on outgoing bubbles.
- Not verified live: real TGV files from Telegram, `getBackgroundUrl` against the server, and Share's clipboard. No real account's wallpaper was changed.
