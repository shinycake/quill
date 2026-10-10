# Appearance 2: system accent, font family, power saving, middle-click quick action

Earlier appearance work (wallpapers, chat themes, interface zoom, custom accent) is in `codex-appearance-wallpapers.md`, `codex-chat-wallpapers-themes.md` and `codex-interface-scale-full.md`. This note covers what came after.

## What tdesktop does
- `settings/sections/settings_chat.cpp`: a "System accent color" option next to the accent circles, shown only when `Window::Theme::SystemAccentColor()` has a value. "Font family" opens `ChooseFontBox` (searchable list, Default and System entries) and restarts the app. "Chat list quick action" runs on swipe and on middle-click (`dialogs_inner_widget.cpp`, `Qt::MiddleButton`).
- `settings/settings_power_saving.cpp` and `ui/power_saving.h`: eleven flags (animations, stickers panel and chat, emoji panel, reactions, chat and status, chat background, spoiler, calls, effects). A checkbox is checked while the animation plays. An extra switch turns everything off while the OS battery saver is on.
- `window/window_adaptive.cpp`: "Adaptive layout for wide screens" caps the history column and aligns every message left once the chat is 880 px wide.

## What changed
- `src/system_accent.rs`: the OS accent. macOS reads `NSColor.controlAccentColor` converted to sRGB. Windows calls `DwmGetColorizationColor`. Linux asks the freedesktop portal (`org.freedesktop.appearance` `accent-color`, through `gdbus`) and falls back to GNOME's named accent (`gsettings`). The parsers and the choice between custom and system colour are pure and tested. A "System" swatch appears only when the OS reports a colour. It is re-read when the dialog opens, when the window is activated and once a minute, and only while the option is on.
- `src/font_choice.rs` and `src/ui/appearance_power.rs`: a searchable font list (kit `Select`) with Default first. It sets the kit theme's `font_family`, so it applies live, no restart. Code blocks keep their monospace family. A stored font that is no longer installed falls back to the default and the dialog says so; the name stays in the prefs.
- `src/power_saving.rs`: the flag set with tdesktop's bit values, kept in an atomic. Switches offered, and what they gate:
  - Stickers in panel and in chat, emoji in panel and in messages (and chat-list previews): the animated clip is not requested, so the existing still image shows and the frame clock gets no ticks.
  - Spoiler effect: specks hold one frame and request no redraws (`spoiler_fx::specks_fps` is 0).
  - Calls: the answer ring, the connecting arc and the group-call halo are drawn still.
  - Interface animations: `Glide::go` snaps, new-message reveal does not start, and GPUI's reduced motion is turned on (`reduce_motion_now`: the OS preference or this switch), which also settles kit springs and `with_animation`.
- Middle-click on a chat row runs the quick action (same handler as the swipe, so the Delete confirmation and chat gating are unchanged).
- Prefs: `system_accent`, `font_family`, `power_saving` in `appearance_prefs.json`, sanitised on load.

## Skipped, and why
- Themes (Day, Classic, Tinted, Night, custom and cloud themes, theme editor): needs a theme file format, an editor and a palette model beyond the kit's tokens. Too big for this slice.
- Interface language packs: Quill has no string table; every UI string is a literal. Extracting them is its own project.
- Adaptive wide layout: tdesktop left-aligns outgoing bubbles too, which touches every bubble builder (`session_bubble_*`, albums, media, service rows). A centred column alone would not match, so nothing was half done.
- Power saving flags with nothing to gate (wallpaper rotation, reaction menu, emoji status, message effects) are not offered. The automatic "save power on low battery" switch is not done: it needs a battery-saver probe per OS (`NSProcessInfo`, `GetSystemPowerStatus`, power-profiles-daemon) that I could not verify here.
- `updates-theme-colors`: not touched.

## Verified
- Unit tests: `power_saving` (bits, groups, sanitising, forced and reduced-motion rules), `system_accent` (portal output, GNOME names, DWM alpha, custom versus system), `font_choice`, prefs round trip and sanitising.
- Demo `ready-appearance-power` in light and dark (fixture accent and font, since both depend on the machine), viewed: the Georgia font applies across the dialog and the app, the System swatch recolours the accent, the switches show the fixture state.
- macOS accent read compiles and runs here. The Windows and Linux readers were written against the documented APIs and could not be compiled or run on this machine; CI builds them, nothing exercised them live.
- Not observed: the animations themselves with a switch off in a live window (the gating is at the points that request frames or decode clips), middle-click on a real chat list.
