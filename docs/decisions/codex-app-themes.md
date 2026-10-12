# App themes: Quill Light and Dark, Classic, Day, Tinted, Night, accents, custom themes and the palette editor

`parity:appearance-themes`. Before this, Quill had a light and a dark palette (GitHub Primer colours for Quill's tokens, shadcn neutral for the kit), one accent colour and a high-contrast mode.

## What tdesktop does

- `window/themes/window_themes_embedded.cpp`:
  - `EmbeddedThemes` lists four themes. Classic is the default palette (`lib_ui/ui/colors.palette`). Day is `day-blue.tdesktop-theme`, Tinted is `night.tdesktop-theme` and Night is `night-green.tdesktop-theme`. Each theme also has a base accent and preview colours.
  - `DefaultAccentColors` gives each theme eight accent circles.
  - `ColorizerFrom` recolours a palette around a new accent: hue threshold 15, accent lightness at most 160 on day themes and at least 64 on night themes, and `kColorizeIgnoredKeys` never moves.
- `settings/sections/settings_chat.cpp` `SetupDefaultThemes`:
  - The four themes appear as preview tiles. The `ColorsPalette` row shows the applied theme's accent, its eight others and a custom circle that opens a colour editor with the lightness limits.
  - "System accent color" is a checkbox, shown only where the OS reports an accent.
  - Choosing a theme of the other kind toggles night mode.
  - "Choose from file" applies `.tdesktop-theme` and `.tdesktop-palette` files. "Create new theme" sits in the section's top-bar menu.
- `window/themes/window_theme.cpp`:
  - A `.tdesktop-theme` is a zip holding `colors.tdesktop-theme` (or `colors.tdesktop-palette`) and an optional `background.jpg`/`.png`, or `tiled.jpg`/`.png`. A `.tdesktop-palette` is the bare palette text.
  - The palette is a list of `key: #rrggbb[aa];` or `key: otherKey;` with `//` and `/* */` comments, read strictly: a syntax error refuses the whole theme. A bad colour value is skipped. Limits: 5 MB per file, 1 MB per palette, 4 MB per background.
  - Keys a file does not set keep the default palette's value. Custom themes get no accent colorizing (`ColorizerForTheme` only handles embedded themes).
  - Night mode keeps a day theme and a night theme and switches between them (`reapplyWithNightMode`, `readThemeAfterSwitch`). The default night theme is Tinted.
  - "Auto-night mode" follows the system (`systemDarkModeEnabled`). A theme counts as dark when `dialogsBg` has an HSV value under 0.5.
- `window/themes/window_theme_editor.cpp` ("Edit color palette"): every palette key with a search field. A row opens a colour box. The menu has "Export theme" (a file dialog that writes a `.tdesktop-theme`), "Import theme" and "Show palette file". "Save theme" uploads a cloud theme.
- `data/data_cloud_themes.cpp`: cloud themes come from MTProto `account.getThemes` / `account.getTheme`.

## What changed

### Core: `src/app_theme/` (tested on Linux CI)

- `keys.rs` holds the 27 palette keys Quill paints with. They use tdesktop's names, so a theme file sets them directly. Each key has an editor label and a group.
- `embedded.rs` defines six built-in themes.
  - **Quill Light and Quill Dark** come first and are the defaults (owner decision). They are drawn by Quill's own token pairs and the kit's default colours, exactly as before; their palette (from the token pairs) only describes them for the preview tile and "Create new theme". Their accent is the old single `accent_rgb`, which re-points only the kit's primary colours, and their presets are the seven old ones. They have no chat background.
  - **Classic, Day, Tinted and Night** are tdesktop's, with palettes resolved from its files for those keys, base accents, the eight accent circles, preview colours and chat backgrounds.
  - Classic's background is the default Telegram wallpaper's four colours (`WallPaper::ConstructDefault`) without its pattern.
  - The other three themes use the colour of their one-pixel `background.png`.
  - `palette_with_accent` runs tdesktop's colorizer (`theme_colorize::Colorizer::from_accent`, already ported for chat themes) over every key except `boxTextFgError`, and over the background.
- `file.rs` parses `.tdesktop-theme` zips and `.tdesktop-palette` text with tdesktop's rules and limits, follows key references, and exports a zip.
  - Every `key: value` pair is kept in order, so a theme Quill imports and exports again keeps the keys Quill does not use.
  - `stat_chart::xlsx::zip` became `pub(crate)` to write the zip. Reading uses `flate2`.
- `png.rs` writes and reads the one-pixel PNGs tdesktop's own themes use for a solid background.
- `store.rs` keeps custom themes as `<settings root>/themes/<name>.tdesktop-theme`. It cleans names for every file system, numbers duplicates ("Ocean Blue 2"), writes through a temporary file and a rename, and imports with a "sets no colours Quill uses" check.
- `ThemeId` is a built-in theme or a custom name. It is stored as `"quill_light"`, `"quill_dark"`, `"classic"`, `"day"`, `"tinted"`, `"night"` or `"custom:<name>"`, and read leniently, so a hand-edited value never resets the other prefs.
- `AppearancePrefs` gains `day_theme` (default Quill Light), `night_theme` (default Quill Dark) and `theme_accents` (per tdesktop theme, like its `themesAccentColors`). `accent_rgb` stays the accent of Quill's two themes, so existing users keep theirs. Prefs saved before this load onto Quill Light and Quill Dark.

### UI: `src/ui/app_theme/`

- For Quill Light and Quill Dark (and high contrast) nothing is installed: `apply_appearance` takes its old path (token pairs, kit theme with the accent override on primary), there are no chat defaults and no theme wallpaper. Outgoing bubbles stay `accent_strong` with white text.
- For any other theme, the applied palette is installed process-wide in atomics (one per thread in tests, like the light/dark flag).
  - `chat_theme` tokens that map to a key read it through `pick_key`: accents, text, surfaces, borders, danger, incoming bubbles and the muted badge.
  - Tokens with no key (warnings, success fills, scrims, peer colours) keep their dark or light value, picked by the theme's darkness.
- `kit_colors.rs` writes the palette into the kit's `ThemeColor` in `apply_appearance`, so kit components follow the theme. Semantic hues the palette has no key for (success, warning, info, charts) keep the kit's values.
- `apply_appearance` resolves the theme for night mode on or off, from the manual choice or auto-night (off, system or scheduled; Scheduled is Quill's existing option). The kit mode follows the palette's darkness, which matters for a dark custom theme in the day slot.
  - The re-apply key is now the palette's fingerprint, so the minute tick stays free.
  - High contrast keeps its own palette and wins over every theme, as before.
- Chats without an emoji theme get the defaults of a tdesktop or custom theme:
  - outgoing bubbles in `msgOutBg`, with black or white text by the fill's lightness;
  - links and names in them shifted to `msgOutServiceFg` through the existing chat-palette code;
  - the theme's background when no wallpaper is chosen.
  - An emoji theme (#708) still wins. `ChatPalette::build` skips the accent colorizer when the accent is the palette's own, so incoming rows pay nothing.
- The Appearance dialog's "Theme" radio and the old accent presets are replaced by a Themes section:
  - six preview tiles, Quill Light and Quill Dark first; tdesktop's are recoloured by their accent as in its `ColorsFromScheme`;
  - the applied built-in theme's accent circles (its own accent first, which for Quill's themes means "default") plus a custom colour field, and "System accent color" where the OS has one;
  - custom themes as tiles, with "Import theme file…", "Create new theme", and Edit, Export… and Delete for the applied one;
  - a "High contrast" switch.
  - Choosing a theme while auto-night would switch straight back turns auto-night off and says so, the outcome of tdesktop's "Disable" answer to its auto-night warning.
- `editor.rs` is "Edit color palette", a kit dialog without a dim, so the app behind shows the changes:
  - a name field, search over labels and key names, and the keys by group plus "Chat background";
  - the selected row holds the kit `ColorPicker`, and every committed colour applies live;
  - "Save theme" stores a custom theme and applies it, "Export…" writes a `.tdesktop-theme` anywhere, and "Cancel" (or Esc) reverts;
  - "Create new theme" starts from the colours on screen, as tdesktop's does.
- Copy follows the app-name rule: the chooser names the file format (`.tdesktop-theme`), not the app.

## Choices

- **Quill's palettes stay the defaults (owner decision).** They are built-in themes listed first, not palettes: drawing them through the old code path is the only way to guarantee nobody who keeps the defaults sees a change, including the kit's own colours, which no palette key covers. tdesktop's four are extra choices.
- **27 keys, not all ~470.** Only keys that something on screen reads are listed and applied, so editing a row always changes the app. The rest stay in the file for export.
- **Cloud themes are blocked.** TDLib 1.8.68 has no API for `account.getThemes`, `account.getTheme`, `createTheme` or `uploadTheme`; `internalLinkTypeTheme` is documented as unsupported (see `parity:deeplink-bg-theme`). So there is no cloud list, no `addtheme/` links and no upload on "Save theme": Save stores the theme locally.
- **Theme backgrounds are one colour.** A solid one-pixel background is read exactly. A photo background becomes its mean colour (decoded with `image`). Classic's default wallpaper pattern is not drawn. Export writes the background as a one-pixel PNG. Tiled photo backgrounds are not reproduced.
- **Not ported:** tdesktop's `keepContrast` pairs for night accents (they only matter for keys Quill does not use), "Show palette file", the editor's "Not in the palette yet" section (every Quill key is always listed), and the import button inside the editor (import lives in the chooser).

## Verification

- Core tests (`src/app_theme/tests.rs`, `keys.rs`):
  - the embedded palettes against tdesktop's values;
  - accent colorizing (hue moves, greys, alpha and ignored keys stay, night lightness floor);
  - strict parsing (comments, references, syntax errors, bad hex);
  - a zip export → parse → palette round trip, including unknown keys and the background;
  - tdesktop-style archives, PNG round trip, `ThemeId` serde, and store import, naming and delete.
- Settings test: legacy prefs load onto Quill Light and Quill Dark, the old accent stays on Quill's themes only, and custom slots round-trip.
- UI tests:
  - kit colours from a palette, the installed palette per thread, chat defaults, tile recolouring and editor search;
  - on a demo app (`ui_tests.rs`): Quill Light and Quill Dark install no palette, keep no chat defaults or theme wallpaper, keep `accent_strong` and the kit primary as before, and the old accent still re-points the primary; Night drives the kit background, `accent_strong` and chat defaults; an accent recolours Classic; high contrast installs nothing; the editor previews a changed key and background live, follows `dialogsBg` for darkness, and Cancel reverts.
- One-off check, not committed: the real `night`, `night-green` and `day-blue` `.tdesktop-theme` files from the tdesktop repository parse to exactly the embedded palettes (Day's missing keys fall back to Classic, as in tdesktop).
- No visual change by default: `ready-chats` captured in light and dark from `origin/main` and from this branch are byte-identical PNGs; `ready-showcase` differs only in its relative timestamps (captured a minute apart; main's own two runs are identical).
- Demo captures to a scratch folder, looked at and not committed: the chooser with the six tiles and Quill's accent circles; the chat list and history in Classic, Day, Tinted and Night; the Appearance chooser in Classic and in Tinted with a pink accent and an imported custom theme; the editor over the Day theme.

## Cost

No performance goal, so no timings. `pick_key` adds one relaxed atomic load per token call. Under a tdesktop or custom theme, rows of outgoing messages in chats without an emoji theme go through the outgoing colorizer, as rows in themed chats already did; Quill's themes are unchanged.
