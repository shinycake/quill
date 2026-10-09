# Pattern wallpapers, per-chat wallpaper and chat themes, wallpaper from file, bg links

## What tdesktop does
- `ui/chat/chat_theme.cpp`: a pattern is a PNG or TGV (gzipped SVG) tile scaled to the area height, repeated across the width with an odd column count so one tile is centered. Positive intensity blends the pattern over the fill with soft light (inverted to white when the fill is dark, `IsPatternInverted`); an inverted pattern (negative intensity, TDLib `is_inverted`) shows the fill only through the pattern with black elsewhere.
- `window/window_peer_menu.cpp` `addThemeEdit`: "Change colors" only for private chats with a user. `ChooseThemeController` picks an emoji theme, then `BackgroundBox` / `background_preview_box` picks a wallpaper "for me" or, with Premium, for both.
- `bg/` links open the wallpaper preview; `addtheme` links install a desktop theme file.

## What changed
- `src/ui/wallpaper_pattern.rs`: resvg (already in the lockfile through GPUI, `default-features = false`) rasterises the SVG/TGV (usvg unzips it); PNG patterns use the `image` crate alpha. The 512 px tile is composed with the intensity, cached in `Lru` (6 entries, retired through `image_budget`), and painted with `canvas` + `paint_image` in tdesktop's tiling.
- Chat wallpaper and theme: `chat.background`, `chat.theme`, `updateChatBackground`, `updateChatTheme`, `updateEmojiChatThemes` are parsed and reduced (`Session::chat_wallpaper`: own wallpaper, else the theme's, else the account's). A chat shows its wallpaper, dark-mode dimming (black overlay), and the theme's outgoing bubble color (dark text on light fills).
- Dialog "Chat colors and wallpaper" (info panel of a private chat, not Saved Messages): theme chips, installed wallpapers, preview. Apply sends `setChatTheme` / `setChatBackground` (`only_for_self`; "also for X" only with Premium) / `deleteChatBackground`.
- "From file..." in Appearance: `setDefaultBackground` with `inputBackgroundLocal` (JPEG, PNG, WebP), picker through `cx.prompt_for_paths` (cross-platform).
- `bg/` links: `searchBackground`, preview dialog, "Set as wallpaper" for the current theme. `addtheme` still says it is unsupported, with the reason (desktop theme files).

## Honest choices
- Soft light has no GPUI blend mode; a normal overlay of black (white over dark fills) at the pattern opacity stands in. The inverted case uses `alpha = 1 - coverage * intensity`. The match to Qt is approximate.
- Not done: blur and motion (no cheap blur path; motion needs a gyroscope), tiled photos, gift chat themes (`chatThemeGift` needs `getGiftChatThemes`), groups and channels (need boosts), accent color from themes, per-chat wallpaper from a local file.
- Freeform gradients still paint as two stops.

## Verified
Unit tests: parsers, requests, reducer precedence, pattern rasterising (SVG, TGV, PNG), tile alpha, tiling columns, change planning, deep link routing. Demo capture `ready-chat-look`, `ready-chat-theme`, `ready-background-link` in light and dark, viewed. Unverified: live Telegram (real TGV files, server errors, the Premium path, `updateEmojiChatThemes` timing); apply errors are not surfaced in the chat dialog.
