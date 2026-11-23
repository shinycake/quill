# Glass effects: the switch, the rules and the surfaces

Settings › Appearance › **Glass effects** (tagged Experimental, off by
default) turns Quill's floating surfaces into frosted glass drawn by the
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
- **Surfaces ask, they never decide.** A surface calls
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

Message bubbles, chat rows, wallpapers, service messages and bot buttons
never get glass (Apple's rule: glass is for controls above content).

## Recipes

Every look is in `src/ui/glass/recipes.rs`. Larger surfaces carry more tint:
menus 0.86 frost, panels 0.90, floating controls 0.84, media text plates
0.55, media icons 0.45 (over black, with white labels). The test beside it
settles each recipe over pure black and pure white, in the light and dark
themes, and requires 4.5:1 for primary and secondary text and 3:1 for
icon-only roles. High contrast never draws glass.

When glass can't be drawn (a lens inside a paint layer, past the last band,
or an unreadable frame) the lens draws its fallback quad in the glass
colour. At 0.86 frost the fallback is about 94% opaque, close to the solid
popover.

## Not done, and why

- **The floating sidebar header (Bar).** See "Budget" for the numbers.
- **PR 4 of the plan** (the history under a floating top bar and composer,
  the send button as the one tinted control) waits for Idan's decision.
- **Rim light that follows the pointer** and the **96% scale-in** when a
  menu opens: GPUI has no element scale, and the rim light needs a pointer
  feed into every lens; both are polish for later.
- **Underline folder tabs** keep their underline: there is no capsule to
  turn into glass.
- **Picture-in-picture** has no filled control to turn into glass.
- **Inked labels and the sharp loupe** (Metal-only extras of the primitive)
  are not used, so the three platforms look the same.

## Budget

BUDGET_TABLE

## Verification

VERIFICATION
