# Interface scale for the whole UI

## What tdesktop does
- Every style metric goes through `style::Scale`, so text, avatars, row heights, paddings and icons grow together.
- `settings/sections/settings_main.cpp` (`setScale`): changing the scale shows a preview (`settings_scale_preview.cpp`) and then asks to restart.

## Before
PR #537 scaled the kit's rem size (`codex-appearance-wallpapers.md`). Text and rem spacing grew. Everything Quill sizes in `px()` stayed the same size: avatars, chat rows, the sidebar, bubble padding, media and icons. About 1,240 `px(` calls in `src/ui` plus `f32` constants, and gpui-kit components that use fixed pixels internally.

## Options considered
- (a) A `s(px)` helper and a scripted rewrite of every `px()` call. It still misses the kit's own fixed sizes (switches, scrollbars, dialog widths) and any `px()` added later. Some `px()` calls are ratios (`x / px(1.)`) or physical-pixel math, so a script would need a hand review of every hit.
- (b) A patch to the vendored platform crates that multiplies the window's scale factor. It covers everything, but it means three vendored patches, and the GPUI 0.3.8 upgrade will re-vendor those crates.
- (c) The same idea without a vendored patch. GPUI takes the platform as `Rc<dyn Platform>` (`Application::with_platform`), and `Platform` and `PlatformWindow` are public traits. Quill wraps the OS platform in a decorator. This is the one Quill uses.

## How it works (`src/ui/interface_zoom.rs`)
`ZoomPlatform` forwards every `Platform` call. `open_window` wraps the real window in a `ZoomWindow`, which reports:
- `scale_factor()` = display scale × zoom. GPUI rasterizes glyphs, SVG icons and images at the real device resolution and snaps layout to physical pixels, so text stays sharp at any scale.
- `content_size()`, `visual_viewport_bounds()`, `mouse_position()` and every input event position divided by zoom. Pixel scroll deltas are divided too, so content still follows the fingers on a trackpad. Line deltas and keys pass through.
- Geometry going back to the OS multiplied by zoom: `resize`, `show_window_menu`, the Linux client inset, input region and exclusive zone, `update_ime_position`, and the minimum window size. macOS traffic lights are moved so they stay centered on the zoomed title bar, since the buttons themselves don't scale.

The scene is already in device pixels, so `draw` passes straight through. Window bounds and displays are screen geometry and are not touched: the window keeps its size on screen and its content scales. Every platform behaves the same way because the decorator sits above them.

## Changing the scale live
The decorator keeps GPUI's resize callback. `set_zoom` updates each open window's zoom and calls that callback. GPUI then re-reads the viewport and scale factor (`Window::bounds_changed`) and redraws, which is the same path a real window resize takes. `apply_appearance` defers the call (`cx.defer`) because GPUI can't update a window that is already being updated. There is no restart prompt, unlike tdesktop. At startup the stored scale is set before the first window opens, so the first frame is already zoomed.

The rem size goes back to the kit default (16 px). Otherwise text would be scaled twice.

## Caches and measured layouts
- Layout is still in GPUI logical pixels and they don't change with zoom. A zoom change looks to the layout like a window resize (the logical width shrinks), and the virtual lists and the message scroller already handle resizes. Chat-list row heights are fixed logical values that scale with everything else.
- Glyph and SVG atlas entries are keyed by the scale factor, so a new zoom rasterizes new ones.
- Avatars and photos (`image_budget::sized_image`) key their decode size on `window.scale_factor()`, which now includes the zoom, so they re-decode sharp.
- Inline video and round players decode at the display scale. `frame_start` now restarts them when the scale changes, so they don't keep the old size.
- Kept as is: animated stickers (128 px) and custom emoji (56 px) decode at fixed sizes regardless of display scale, to stay within their memory budgets. Doubling the sticker edge would let one 120-frame sticker fill the whole 48 MB budget. The 512 px wallpaper pattern tile is also unchanged. All of these get softer at high scales, the same way they already were on a 2x display.

## Known gaps
- IME candidate window on macOS and Windows: those platforms place it with `PlatformInputHandler::bounds_for_range`, a GPUI type whose handler the decorator can't wrap. At scales above 100% the candidate box opens closer to the window's top-left corner than the caret. Linux (X11 and Wayland) and the Windows caret path use `update_ime_position`, which the decorator scales. Fixing it needs one multiply in gpui-base's input element (already vendored), which is best done after the GPUI 0.3.8 upgrade.
- The call panel picked its compact layout from the window's screen size. It now uses the viewport, which shrinks with the scale.
- A GPUI upgrade that adds a trait method with a default body would compile without the decorator forwarding it. `forwards_every_trait_method` reads the trait definitions from the GPUI source in the cargo registry and fails when a method isn't forwarded.

## Measured
Debug demo-capture build, `ready-chats`, 1200x800 window on an M1 Pro, RSS three seconds after the ready marker, two runs each: 100% 131 / 136 MB, 150% 136 / 135 MB, 200% 136 / 134 MB. The difference is within run-to-run noise. At 100% every call is a plain forward.

## Verified
- Unit tests: percent to zoom, geometry round trips, pointer, scroll and file-drop mapping, traffic-light centering, live relayout of open windows (closed windows skipped, unchanged zoom is a no-op), and trait-forwarding coverage.
- Demo captures (`QUILL_DEMO_INTERFACE_SCALE`, English fixtures): `ready-chats` at 100%, 150% and 200% with the window scaled in proportion (700x480, 1050x720, 1400x960) give the same layout at 1x, 1.5x and 2x with sharp text. The same kind at a fixed 1200x740 window at 150% and 200%, `ready-appearance` (settings) at 100%, 150% and 200%, `ready-media` and `ready-chat-list` (context menu open) at 150%: nothing clipped beyond what a narrow window clips anyway. `ready-appearance-wallpapers` opens at 100% and switches to 125% after the window is up, which exercises the live path.
- Not verified on Linux or Windows hardware. The decorator is platform-independent, and CI builds both.
