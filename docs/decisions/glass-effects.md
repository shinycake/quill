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
| Card | the Floating layout's cards: the chat list card, the header-and-bars card, the composer card, the side-panel card and the left tab column. The liquid look (2026-11-26, after #1508): the bar's 30 px pull gathered at the rim, a quiet rim light (0.35, no glowing frame) and the bar's frost and 8 px blur under the rows (docs/decisions/floating-layout.md, "Glass on the cards") |

Message bubbles, chat rows, wallpapers, service messages and bot buttons
never get glass (Apple's rule: glass is for controls above content).

## Recipes

Every look is in `src/ui/glass/recipes.rs`, and there is one look: clear
glass. Idan picked it from three candidates on 2026-11-23, after the first
recipes (one heavy frost from edge to edge, 0.84 to 0.90) read as frosted
rather than liquid.

- **One even pane, after the source kit's frosted card** (2026-11-25).
  The first clear look kept the rim band clear, saturated (2.0) and sharp
  around a middle frosted over a 14 px blur, with the rim light and
  fresnel at 1.6x and no top sheen. Side by side with the `ready-glass`
  demo (the gpui-liquid-glass presets) it read as a thick glowing border
  around frosted plastic: the clear ring drew the eye, the 10 px pull
  spread over the whole slab bent nothing visibly, and the doubled
  saturation turned a photo's edge neon. The recipes now follow the kit's
  card: the same tint from edge to edge and the kit's rim light, fresnel,
  sheen and caustic, with a faint inner shade (0.25) and a 24 px bevel;
  stronger or wider, a tall menu's corners crease along the diagonals and
  light glass turns grey.
- **As liquid as it goes** (2026-11-25). Idan asked for more liquid and
  picked the clearest of three steps: a 2 px blur on slabs and none on
  buttons, an 80 px radial pull that reaches deep into the slab
  (`bend_curve` 4, `bend_depth` 1.7) so what is behind curves like it is
  under water, saturation 1.3 and a 1.8x top sheen on slabs. Buttons bend
  64 units at the rim, magnify their middle by 0.18 and split colour
  (0.25) at the edge. The middle's frost is 0.56 (light) and 0.49 (dark)
  of what the flat black-and-white rule would need: 0.46 for light menus,
  0.35 for dark.
- **The chat-list bar stays thicker** (0.78 and 0.68 of the rule, an
  8 px blur, a 30 px pull gathered at the rim): it sits over rows of text
  right under its own labels, and thinner glass lets the rows ghost
  through the folder tabs. Its rim light stays low, or its lower edge
  reads as a neon line.
- Media glass keeps the full rule (0.44 behind icons, 0.56 behind text),
  since a video frame can be flat white.
- Light glass draws the theme's hairline border so it doesn't melt into a
  light window; dark glass and media glass draw a faint white 1.3 px one,
  and buttons draw none (their rim light is the edge). Menus and panels
  cast a tight shadow (softness 10, offset 4).

The contrast test beside the recipes holds the limits in both themes:

| Backdrop | Primary text | Secondary text and icons |
| --- | --- | --- |
| Mid grey (mixed content behind the labels) | 4.5:1 | 2.4:1 |
| Flat pure black or pure white | 2:1 | not held |
| Media glass over flat pure black or white | 4.5:1 (icons 3:1) | 3:1 |

This is much weaker than the plan's original rule (4.5:1 over flat black
and white everywhere), and Idan chose it knowing labels sit over a nearly
sharp backdrop. The weak cases: dark glass over a flat white area drops
primary text to about 2.3:1, grey secondary text such as the "Sent
yesterday" line is about 2.5:1 to 2.9:1 over mixed content, and text
behind a menu shows under its labels. Labels that adapt to the glass under
them (the stage 5 inks) would win this back. High contrast and Reduce
Transparency never draw glass.

When glass can't be drawn (a lens inside a paint layer, past the last band,
or an unreadable frame) the lens draws its fallback quad in the glass
colour.

## Tuning

Settings › Appearance › Glass effects shows a tuning panel under the
switch while glass is drawn (2026-11-26, Idan: sliders on the current
look, not the edge look of the closed PR 1509). Each of its 14 sliders is
one knob of the recipes above, named by its value on a menu: Bend (80 px),
Bend depth (1.70), Bend curve (4.00), Frost (56%), Backdrop blur (2.0 px),
Saturation (1.30), Sheen (1.80), Rim light (1.00), Inner shade (0.25),
Edge line (24%), Shadow (100%), and for buttons Button refraction (64),
Button magnify (0.18) and Color split (0.25).

- **One dial moves every role.** A role multiplies its own shipped value by
  the dial's share of its default (`GlassTuning::scale`), so moving Bend
  from 80 to 160 px doubles the menu's pull and the bar's 30 px together,
  and the defaults are exactly the shipped look (a 1.0 factor). Captures of
  a glass menu at the default dials match main within the noise between two
  runs of main.
- **Media glass keeps its frost** whatever the Frost dial says: a video
  frame can be flat white. The contrast test holds the limits for the
  default dials only; past them the person sees what they chose.
- The dials are whole stored steps in `AppearancePrefs::glass_tuning`
  (`quill::glass_tuning`), clamped to their ranges when read, so a
  hand-edited file can't push the shader past the sliders. A drag redraws
  every window at each step and saves on release; Reset puts every dial
  back. A live preview (a menu and two buttons over colour bars) sits above
  the sliders. Demo: `ready-settings-glass-tuning`, with
  `QUILL_DEMO_GLASS_DIALS=bend=150,frost=15` to move dials.

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
clear looks keep the same lenses, bands and eight-tap frost and only
lower the blur (14 px, then 6 px, against 18 to 22), so their cost should
not be higher, but they were not measured again. These are relative numbers for this decision,
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
