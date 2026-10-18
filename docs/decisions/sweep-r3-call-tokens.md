# Call window colours follow theme tokens

## What tdesktop does

The call window and the group call panel use their own palette keys
(`callBg`, `callNameFg`, `callStatusFg`, `callIconBg`, the `groupCall*`
set). They are dark in every theme, light ones included, because the
pictures sit on the stage and the controls have to read over them.

## What changed

- New `src/ui/app_theme/call_chrome.rs`: one token set for the call chrome
  (surface, raised rows, menu surface, muted text, borders, glass buttons,
  chips, scrim, shades, washes, danger tint and fill). Normal themes keep
  the exact values the literals had, so light and dark captures are
  unchanged. Under high contrast (`chat_theme::high_contrast`) the same
  roles re-root: pure-black surfaces, white text and borders, heavier
  scrims, opaque danger fills.
- Replaced the literals in `call_panel_devices.rs`, `call_panel.rs`,
  `group_call_panel.rs`, `group_call_tiles.rs` and the feature icon in
  `premium_ui/promo.rs` (`text_on_fill()`).
- The pure-black video letterbox in the tiles stays `black()`.
- The `u32` constants other screens import from `group_call_panel` (screen
  chooser, stream view, video player overlay) now point at the same
  `call_chrome` values and keep their old behaviour; they do not adapt yet.
- Demo hooks (demo-capture only): `QUILL_DEMO_DEVICES=1` opens the device
  menu on `ready-video-call`, and the call demos honour
  `QUILL_DEMO_THEME=high-contrast` (the generic demo prefs only knew dark).

## Why not a light call window

A light stage would put dark text on arbitrary video. tdesktop does not do
it, so the adaptation is high contrast only.

## How verified

Unit tests for `call_chrome` (muted text reaches AA on each surface in
normal and high contrast; high contrast never lowers a contrast ratio).
Before/after captures of `ready-video-call`, the device menu and the group
call window with tiles (`ready-call-screen-share`), in light, dark and
high contrast: light and dark are pixel-equivalent apart from the live
video; high contrast now shows a black menu with a white outline, white
status text and heavier chips.

## Contrast change

The group panel's secondary text (`0x91979e` on the `0x2c333d` rows) was
4.3:1, under AA. The panel now draws it at `0xa3a9b0` (4.8:1 on the hover
row). Other screens that import the old constant are unchanged.
