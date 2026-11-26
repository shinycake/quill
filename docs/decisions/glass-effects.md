# Glass effects: the switch, the rules and the surfaces

Settings › Appearance › **Glass effects** (tagged Experimental, off by
default) turns Quill's floating surfaces into clear glass drawn by the
backdrop lens primitive (docs/decisions/backdrop-glass.md). With the switch
off Quill draws exactly what it drew before: no lens primitive, no pipeline,
no atlas and no readable frame.

The feature is labelled Experimental because glass has only run on a real
GPU on macOS. Linux and Windows run it on CI's software drivers (lavapipe,
llvmpipe, WARP) with the look forced, and real hardware there is unchecked.

## The one source of truth

- **The choice** is `Appearance::material` (`quill::glass::Material::{Solid,
  Glass}`), saved in appearance_prefs.json with the other appearance
  choices. Older files read as `Solid`.
- **The look** is `quill::glass::resolve(material, readings, override)`, a
  pure function that returns `GlassLook::Solid(reason)` or
  `GlassLook::Glass(motion)`. The rows, first match wins:

  | Reading | Look | Caption under the switch |
  | --- | --- | --- |
  | Switch off | `Solid(Off)` | what glass does |
  | Reduce Transparency (macOS), transparency effects off (Windows) | `Solid(ReduceTransparency)` | Paused while Reduce Transparency is on / while transparency effects are off |
  | High contrast theme in Quill, Increase Contrast (macOS), a high-contrast theme (Windows, GNOME) | `Solid(HighContrast)` | Paused in high contrast |
  | Battery saver or a hot system (`quill::battery_saving::forcing`) | `Solid(SavingPower)` | Paused to save power |
  | A software graphics adapter (`Window::gpu_specs().is_software_emulated`) | `Solid(Unsupported)` | Not available on this graphics driver |
  | Reduce Motion | `Glass(Still)` | what glass does |
  | Otherwise | `Glass(Full)` | what glass does |

- **The app global** `ui::glass::GlassState` holds the choice and the
  readings, and stores the resolved look. Three writers feed it
  (`set_choice` from `apply_appearance`, `set_system` from the system
  probe, `refresh` when Reduce Motion or power saving changes), and every
  write resolves again and redraws the windows only when the look changed.
- **Surfaces** only ask. A surface calls
  `glass_or(role, cx, solid)` (or `glass_button` / `media_glass_button` /
  `GlassFill`), which draws the role's recipe while the look is glass and
  the old solid styling otherwise. The kit's popovers, toasts and the
  segmented tab selection ask through the `PopoverGlass` hook, which Quill
  installs the first time the look resolves.
- `QUILL_GLASS=off|on|force` overrides the switch for demos and CI; `force`
  also skips the software adapter row so CI can draw glass on lavapipe and
  WARP. `QUILL_REDUCE_TRANSPARENCY` and `QUILL_INCREASE_CONTRAST` stand in
  for the system settings in demos.

## The surfaces

The list is closed. A new glass surface needs a role and a place in the
budget.

| Role | Surfaces |
| --- | --- |
| Menu | context menus and submenus (`MenuSurface`), kit popovers and selects, the mention and suggestion menus |
| Panel | the main menu grid, the emoji/sticker/GIF panel, the reaction bar, status toasts and kit toasts |
| Control | the jump-to-latest and jump-to-mention buttons, the floating date pill |
| MediaIcon | the media viewer's arrows, the story viewer's arrows, the plain call buttons |
| MediaText | the media viewer's docked bar, captions, the video controller |
| Selection | the Chats/Contacts/Calls selection, which slides as clearer glass |
| Bar | the chat list's header, folder tabs and search field, floating over the rows (Classic only: in Floating the list card is the glass) |
| Card | the Floating layout's cards: the chat list card, the header-and-bars card, the composer card, the side-panel card and the left tab column. Frosted to the edge, barely bent or lit, a wider blur (docs/decisions/floating-layout.md, "Glass on the cards" and "Appearance review round") |

Message bubbles, chat rows, wallpapers, service messages and bot buttons
never get glass (Apple's rule: glass is for controls above content).

## Recipes

Every look is in `src/ui/glass/recipes.rs`, and there is one look: clear
glass. Idan picked it from three candidates on 2026-11-23, after the first
recipes (one heavy frost from edge to edge, 0.84 to 0.90) read as frosted
rather than liquid.

- **The rim band is clear.** The lens primitive gained
  `LensTune::rim_frost`, which thins the frost across the rim band on every
  backend (Metal, Vulkan/GL, DirectX). The rim shows the backdrop bent
  (a 10 px radial pull on slabs, edge refraction on buttons), saturated
  (2.0) and nearly sharp, with a lit edge.
- **The middle, where labels sit, carries a light tint over a strong blur**
  (14 px). Its frost is 0.78 (light) and 0.68 (dark) of what the flat
  black-and-white rule would need: 0.65 for light menus, 0.49 for dark.
  Media glass keeps the full rule (0.44 behind icons, 0.56 behind text),
  since a video frame can be flat white.
- Light glass draws the theme's hairline border so it doesn't melt into a
  light window; dark glass and media glass draw a faint white one. Menus
  and panels cast a tight shadow (softness 10, offset 4). The chat-list
  bar keeps its rim light low, or its lower edge reads as a neon line.

The contrast test beside the recipes holds the limits in both themes:

| Backdrop | Primary text | Secondary text and icons |
| --- | --- | --- |
| Mid grey (what the blur makes of mixed content) | 4.5:1 | 3:1 |
| Flat pure black or pure white | 3:1 | not held |
| Media glass over flat pure black or white | 4.5:1 (icons 3:1) | 3:1 |

This is weaker than the plan's original rule (4.5:1 over flat black and
white everywhere), and the trade is known: grey secondary text, such as
the "Sent yesterday" line in a menu, gets faint when light glass sits over
a nearly black photo, and dark glass over a flat white area drops primary
text to about 3.4:1. High contrast and Reduce Transparency never draw
glass.

When glass can't be drawn (a lens inside a paint layer, past the last band,
or an unreadable frame) the lens draws its fallback quad in the glass
colour.

## Not done, and why

- **PR 4 of the plan** (the history under a floating top bar and composer,
  the send button as the one tinted control) waits for Idan's decision.
- **Rim light that follows the pointer** and the **96% scale-in** when a
  menu opens: GPUI has no element scale, and the rim light needs a pointer
  feed into every lens; both are polish for later.
- **A separate scroll edge fade** under the header: the bar's clear,
  bending lower edge already marks where the rows pass under it.
- **Underline folder tabs** keep their underline: there is no capsule to
  turn into glass.
- **Picture-in-picture** has no filled control to turn into glass.
- **Inked labels and the sharp loupe** (Metal-only extras of the primitive)
  are not used, so the three platforms look the same.

## Budget

Measured 2026-11-22 on Idan's M-series Mac (macOS 27), release build with
`demo-capture,frame-profiler`, a 1200 by 800 window at 2x. GPU time is
the command buffer's `GPUEndTime - GPUStartTime` per frame
(`QUILL_GPU_PROBE=1`, `gpui-pre-apple`), first 30 frames skipped; CPU is
the frame profiler's draw histogram. Every comparison ran in ABBA order
(off, on, on, off), twice. The runs used the first, frosted recipes; the
clear look keeps the same lenses and bands and a smaller blur (14 px
against 18 to 22), so its cost should not be higher, but it was not
measured again. These are relative numbers for this decision,
not public claims.

| Situation | Budget | Result |
| --- | --- | --- |
| Switch off | no lens, no pipeline, pixels as main | 0 lenses in the scene test; 24 captures (8 demos, light and dark, menus open) identical to main, except two demos whose captures differ between two runs of main by the same amount |
| Menu open over a still chat, hovering items | at most +0.3 ms GPU per frame | median 2.97 ms off, 3.24 ms on: +0.27 ms |
| First glass after launch | within 4 ms of the solid path | the first menu frame took 3.5 to 5.4 ms of GPU against 3.0 ms for later ones; the tab selection already draws glass on the first frame, so the pipelines are built before any menu opens and no separate prewarm was needed |
| Scrolling a long chat under the jump button and date pill | at most +1.0 ms GPU, +0.3 ms CPU, p99 under 8.3 ms | median 1.50 ms off, 2.20 ms on (+0.7 ms); p99 5.4 to 7.7 ms off, 5.7 to 6.7 ms on; CPU draw medians 2.3 to 5.1 ms off and 4.7 to 4.8 ms on, inside run-to-run spread |
| Scrolling the chat list under the glass header | as above | median 2.65 ms off, 3.45 ms on (+0.8 ms; one of four runs +1.2 ms); p99 4.7 to 9.4 ms off and 5.4 to 11.1 ms on, one outlier each side. Inside the budget with little margin: the bar and the selection over it are two lenses in two bands |
| Video under the viewer's controls | at most +0.5 ms GPU per frame | median 2.10 ms off, 2.13 ms on |
| Power during a 60 s scripted scroll | within 2% or inside run-to-run noise | system load (SMC `AccumulatedSystemLoad`, on AC power) averaged 29.0 W off and 32.2 W on, with runs of the same setting ranging 18 to 39 W: inside noise, and too noisy to resolve 2% |
| Memory | at most +16 MB with glass on screen, back after | footprint 89 MB off, 89 to 90 MB on with a menu open, unchanged after it closed |
| Floating cards, scrolling a chat under them (2026-11-25) | as the scroll row | median 0.98 to 1.00 ms off, 3.26 to 3.38 ms on (+2.3 ms), p99 9.0 to 9.9 ms on: over budget, open for the owner (floating-layout.md) |
| Floating cards with the Card recipe (2026-11-26) | as the scroll row | median 1.29 to 1.37 ms off, 2.89 to 3.39 ms on, p99 10.7 to 11.9 ms on: the same cost as the Panel recipe within noise. Still over budget, with the owner (floating-layout.md, "Glass cost") |
| Amount of glass | at most 8 lenses in 2 bands; always-visible glass under 15% of the window | scene test: a steady main window stays within 8 lenses and 2 bands; the header bar is about 6% of a 1200 by 800 window |

The power row is the weakest. The Mac was on AC power with other work
running, and it never measured battery draw. A clean battery run (four
ABBA rounds at fixed brightness, nothing else running) is still open; if
it shows a cost, the plan's remedy order applies (offscreen frames, then
solid bars on battery).

## Verification

- **Tests.** `quill::glass` resolve rules (seven cases), the recipe
  contrast test in light and dark, and scene tests in a drawn window:
  the solid look paints no lens; an open menu adds exactly one lens and
  switching off removes them all; with the header floating, a click just
  under the bar opens the first row, and after walking up from the end of
  a long list with the arrow keys the row just under the bar is the
  highlighted one (the test fails without the list's top inset).
- **macOS captures** (`/verify`, demo builds): menus, the main menu, the
  emoji panel, toasts, the media and story viewers, video controls, calls,
  jump buttons, the date pill, the tab selection and the floating header,
  in light and dark, Hebrew names, the story pull and the folder tabs, and
  the settings row with its Reduce Transparency caption.
- **Linux and Windows** on CI (a temporary workflow, removed before the
  PR): ready-showcase with a menu open on lavapipe (Vulkan), llvmpipe
  (OpenGL) and WARP (DirectX 11), with `QUILL_GLASS=force` in light and
  dark. All drew the glass menu. With `QUILL_GLASS=on` the same software
  adapters resolve to the solid look, as designed. No real Linux or
  Windows GPU has run it.
