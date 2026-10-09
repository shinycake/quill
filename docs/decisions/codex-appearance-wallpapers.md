# Appearance: interface scale and Telegram wallpapers

## What tdesktop does
- `settings/settings_chat.cpp`: "Interface scale" is a slider from 100% to 300% (step 5) plus a "Default" switch for the system scale; changing it asks to restart.
- `boxes/background_box.cpp` / `background_preview_box.cpp`: a gallery of the account's installed backgrounds (photos, patterns, gradients), a preview box with Blur, Motion and Pattern intensity, "Set for this chat only" and apply. `setDefaultBackground`, `removeInstalledBackground`, and per-chat `setChatBackground` carry the choice.

## Honest choices
- GPUI has no window-wide scale factor. The kit root feeds `Theme::font_size` to `Window::set_rem_size` every frame, so scaling that value scales text and rem-based spacing live (no restart). Sizes written in fixed pixels (avatars, chat row heights, the sidebar width) do not scale; the dialog says so. The 100-300% range is kept, offered as seven steps (100, 125, 150, 175, 200, 250, 300); stored values clamp and snap to steps of 5.
- Telegram wallpapers: `getInstalledBackgrounds`, `setDefaultBackground` (installed background, own type), `removeInstalledBackground`, and `updateDefaultBackground` are wired. Solid and gradient fills paint exactly (freeform gradients paint as two stops), photos paint once downloaded (`ObjectFit::Cover`), pattern wallpapers paint their fill only. Blur, motion and pattern intensity are not drawn (no cheap blur or tinted-mask path in GPUI), and the Appearance dialog says so. The Telegram wallpaper is opt-in (`telegram_wallpaper` in `appearance_prefs.json`) and chosen from the grid; the preset colors and Default turn it off.
- Telegram's gradient `rotation_angle` (0 = top to bottom) maps to a CSS-style angle `(180 + angle) % 360`; unverified against a live gradient.

## Skipped
- Wallpapers from a file, per-chat wallpaper (`setChatBackground`), chat themes (`setChatTheme` / `updateChatTheme`), theme and background deep links (`internalLinkTypeBackground` / `Theme` still say unsupported). Each needs a preview and apply flow of its own.

## Verified
Unit tests (background parsing, requests, reducer, wallpaper resolution, scale clamping); demo capture `ready-appearance-wallpapers` in light and dark. No live account: real photo wallpapers and server errors are unverified.
