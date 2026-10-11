# Name color, profile color and reply icon editor

Parity item `parity:settings-name-color`. Partly done: collectible wear is not.

## What tdesktop does
- `EditPeerColorBox` (`boxes/peers/edit_peer_color_box.cpp`) is a box with a Profile tab and a Name tab, each with a live preview (a profile header, or a message with a reply), a color selector, and an "Add icons" button that opens the custom emoji panel in background-emoji mode.
- The Name tab also has gift tabs and a gift selector: an owned or hosted unique gift can be worn as the name color (`setUpgradedGiftColors` in TDLib terms).
- Apply for your own account is gated: without Premium `ShowPremiumPreview` opens the Premium box and nothing is sent. With Premium it sends one update per changed section and closes the box.
- Entry points: Settings (the "Your color" button) and the `settings/profile-color` deep links.

## What changed
- `src/telegram/requests/users.rs`: `setAccentColor` and `getDefaultBackgroundCustomEmojiStickers` request builders (names checked against `schema/td_api.tl` 1.8.68: lines 15205, 15154, plus `setProfileAccentColor`, `updateAccentColors`, `updateProfileAccentColors`).
- `src/state/domains/users/`: purposes `SetAccentColor` and `GetBackgroundEmojiIcons`; the icon list and a failed flag in `UsersState`; failures of the set request use the existing profile edit error line. The stickers answer is routed by purpose in `stickers/apply.rs`.
- `src/state/domains/chats/`: `updateAccentColors` now keeps `available_accent_color_ids` (the settable name colors, in server order); before, only the palette was stored.
- `src/peer_color.rs` (core, tested): the draft, the Premium gate and which sections an Apply sends, plus swatch lists (built-in seven until the update arrives).
- `src/connect/peer_color.rs`: `set_accent_color`, `set_profile_colors` (both refuse without Premium), `load_background_emoji_icons`, `download_background_emoji_icons`.
- `src/ui/peer_color_dialog.rs`: the box. Tabs, preview, swatches, an icon grid fed by the TDLib default background icons, Reset profile color, Apply (opens Premium without it), result and error line. Registered with `register_dialogs!` at priority 5950 so it shows above Edit profile.
- Edit profile: the old profile-only swatch section is replaced by a "Name and profile color" button that opens the box. The box also saves the profile icon, which the old section always preserved.
- Fix found on the way: `user.profile_background_custom_emoji_id` was read with `as_i64`, but TDLib JSON writes int64 as a string, so the field was always 0 for real accounts.

## Not done
- Collectible wear: Quill does not read `upgradedGiftColors` (the received gifts list has no color data), so there is nothing to choose from. `setUpgradedGiftColors` is left out.
- The `settings/profile-color` deep links and a Settings entry beyond Edit profile.
- Icon thumbnails are static; tdesktop animates them and tints them with the color.
- Channel and group colors (`setChatAccentColor`) are a separate item.

## How verified
- gate.sh: GATE OK. New tests: request JSON, Premium gate and apply outcomes, swatch lists, driver (Premium refusal, request bodies, icon list load once and thumbnail download), error handling, `updateAccentColors` ids, string int64 ids on `updateUser`.
- A throwaway demo capture (not committed) rendered both tabs: preview, swatches, icon grid, selected state.
- Not verified: a live account (applying never ran against Telegram), and clicking through the box.
