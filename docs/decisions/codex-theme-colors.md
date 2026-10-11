# Chat themes recolour the chat; gift themes, dimming, chat wallpaper from a file, gallery thumbnails, rotation on send

Follows `codex-wallpapers-themes.md`, which drew theme backgrounds and the outgoing bubble fill but left the rest of the chat in the app's own colours.

## What tdesktop does

- `window_session_controller.cpp` `cacheChatTheme` builds a chat theme from `PreparePaletteCallback(dark, accentColor)`. That is the day or night palette passed through `ColorizerFrom` (lib_ui `style::colorize`): every palette colour within 15° of the base accent's hue is shifted to the accent's hue, its saturation and value are rescaled, and the accent's lightness is held to at most 160 on day themes and at least 64 on night ones.
- `ChatTheme::adjustPalette`:
  - one outgoing colour sets `msgOutBg`;
  - the bubble accent is `outgoing_accent_color`, or the reply-bar colour toned by the outgoing colours' average (`ThemeAdjustedColor`);
  - `bubblesAccentColorizer` (hue threshold 255) moves every outgoing colour (dates, reply bars, links, icons, file and media colours) to that accent;
  - if any of those colours, or the outgoing text, has a contrast under 1.14 against any outgoing fill colour, they all become black (day) or white (night);
  - `msgServiceBg` and the scroll colours take the hue and saturation of the background's average colour, whatever the wallpaper.
- `chatThemeGift` themes carry their light and dark `themeSettings` inline.
- `BackgroundPreviewBox` for a chat has a dimming slider in dark mode for photo wallpapers. `BackgroundBox` for a chat has "Choose from file" (`uploadForPeer`). The gallery draws photos from document thumbnails.
- `rotateComplexGradientBackground` runs when an outgoing message appears in the open chat (`history_widget.cpp`, `history_view_list_widget.cpp`). It moves a freeform gradient half a phase (`kAddRotationDoubled`) and crossfades the new picture in over 200 ms. Power saving "Wallpaper rotation" (`kChatBackground`, bit 6) turns it off.

## What changed

- `src/theme_colorize.rs` (core): Qt's integer HSV and HSL, `style::colorize` and `ColorizerFrom` with the lightness limits, `bubblesAccentColorizer`, `ThemeAdjustedColor`, and the WCAG contrast tdesktop uses.
- `ui/chat_palette.rs`: a chat's palette, made of the theme colorizer, the outgoing tone (shift, or flat black or white) and the wallpaper tone for service pills.
  - It is set in a thread-local only while the open chat's rows (and the floating date pill) are built. `chat_theme::pick` and `pick_a` pass every token through it, and so does `text_on_fill` inside outgoing bubbles. Links, quote bars, poll and media accents, reaction chips and the jump highlight follow the theme; the chat list, header and composer keep the app colours.
  - Themed outgoing bubbles get black text on day themes and white on night ones, as tdesktop does. The old luminance guess is gone.
- Gift themes: `ChatThemeRef { Emoji, Gift }` replaces the theme name in `chat.theme` and `updateChatTheme`. A gift theme's settings are stored per chat and used like an emoji theme's.
- Chat colours dialog:
  - "From file…" picks an image for this chat (`setChatBackground` with `inputBackgroundLocal`);
  - in dark mode a photo wallpaper shows a dimming slider (0-100, the preview dims live);
  - a dimming change on the chat's own wallpaper re-sends it with the new `dark_theme_dimming`.
- Gallery: `background.document.thumbnail` is parsed. Installed photos download only their thumbnail, and tiles draw from it. The full photo downloads when it is previewed or picked for a chat (`download_background_file`, deduped while in flight).
- Rotation on send:
  - a freeform wallpaper (three or four colours, alone or under a pattern) turns one doubled step per outgoing message appended at the bottom; the rotation is kept per wallpaper identity;
  - `FillSampler::rotated` follows `ComputeRealRotation` and `ComputeRealProgress`, interpolating the colour points between phases;
  - the new picture renders on the wallpaper thread; the history then crossfades from the old one over 200 ms on the frame clock (an overlay at falling opacity);
  - "Wallpaper rotation" (bit 6) is now in Battery and animations; reduced motion swaps without the fade.

## Honest choices

- The colorizer recolours Quill's palette tokens, not tdesktop's palette keys. Quill's greys carry a slight blue tint (GitHub greys), so they pick up a faint theme tint the way tdesktop's coloured greys would. Pure greys and far hues (reds, greens) are untouched.
- The flat fallback replaces every chromatic token (saturation ≥ 40/255) inside the outgoing bubble with the text colour, which is close to tdesktop's list of keys.
- Photo wallpapers do not tone service pills, because their average colour is not known on the UI thread. Fills and patterns do.
- Gift theme backgrounds draw without the gift symbols tdesktop places in the pattern.

## Verified

- Core tests:
  - HSV and HSL against Qt values;
  - colorizer shifts and the lightness limits;
  - the bubble colorizer, `adjusted` and contrast;
  - the freeform rotation steps (16 sends wrap) and that only freeform rotates;
  - the thumbnail parse;
  - the chat-file request JSON;
  - which files the gallery downloads;
  - the power-saving bit;
  - gift and emoji theme parsing and reduction.
- UI tests:
  - the palette is scoped and restored, and greys are untouched;
  - day black and night white bubble text, and the flat fallback;
  - service pill tone;
  - plans for a dimming-only change and for a file;
  - fade timing;
  - rotation per identity.
- Captures (temporary fixture, not committed), all viewed:
  - a light pink theme: pink links in and out, black text on pastel bubbles, toned date pills, the chat list still blue;
  - a dark theme: lavender links, white text on purple bubbles;
  - the freeform gradient after three sends next to the resting one;
  - the chat colours dialog in dark mode with a photo: the dimming slider, "From file…", the dimmed preview.
- Not verified live: real gift themes from the server, uploading a chat wallpaper file, and the crossfade as it plays. Captures are still frames. The fade start, timing and overlay are unit-tested.
