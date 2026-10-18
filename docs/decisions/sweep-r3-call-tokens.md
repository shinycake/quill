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

## Follow-up (sweep-r3-call-tokens-2)

- Group member rows (`group_call_members.rs`) draw "not joined" text with
  `call_chrome::group_muted()` (AA on the raised rows, white in high
  contrast) instead of the old 4.3:1 literal. Story chrome and the video
  overlay still import the legacy constant.
- Premium promo: the white on the brand gradients (feature icon, hero star,
  discount badge, fallback star) is one fixed `on_brand()` white. The
  gradients are fixed brand colours, so custom themes and the themed
  `text_on_fill()` no longer recolour one icon.
- `demo_high_contrast` is `cfg(feature = "demo-capture")`.
- Answer and hang-up buttons: `call_chrome::answer_fill()` / `hangup_fill()`.
  Normal themes keep tdesktop's `0x66c95b` / `0xd75a5a`; high contrast
  darkens them (`0x1a6b2a`, `0xb3261e`) so white reaches 4.5:1.
- Tokens audit: neither gpui-kit's theme colours nor `app_theme` /
  `chat_theme` has a fixed white-on-brand or call-stage token (`text_on_fill`
  follows the installed theme by design), so the new names are the only ones.
- Tests pin every normal-theme token to the old literal and check caption,
  chip icon and white-on-danger contrast in normal and high contrast.

Verification: group call window (`ready-call-screen-share`, three themes)
and unit tests. The demo has no not-joined member and offline Premium promo
data is not injected, so those two screens are covered by tests only.
