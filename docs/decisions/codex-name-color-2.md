# Your color: Settings entry, deep links and collectible wear

Follow-up to `codex-name-color.md`. Finishes `parity:settings-name-color`.

## What tdesktop does
- "Your color" buttons sit in Settings > Appearance (`settings_chat.cpp`) and on the edit-profile page (`settings_information.cpp`).
- Deep links `settings/profile-color[/profile|/name][/add-icons|/use-gift|/reset]`, `appearance/your-color/...` and `edit/your-color` open `EditPeerColorBox` on the Profile or Name tab.
- The Name tab lists the account's collectible gifts that carry colors; picking one clears the color index and icon, and Apply sends the gift colors.

## Is the gift data reachable?
Yes, in TDLib 1.8.68. `getReceivedGifts` has `exclude_without_colors` ("gifts that can't be used in setUpgradedGiftColors") and `exclude_hosted` (left off, so hosted gifts count like in tdesktop). Each `upgradedGift` carries `colors:upgradedGiftColors`, and `setUpgradedGiftColors upgraded_gift_colors_id:int64` applies one. `user.upgraded_gift_colors` says which scheme is worn. Quill already loaded received gifts for the Premium hub but dropped `colors`, so nothing exposed them.

## What changed
- `src/gift_colors.rs`: `UpgradedGiftColors` and its parser (int64 ids sent as strings). `GiftInfo.colors` keeps it, and `ParsedUser.upgraded_gift_colors_id` reads the worn scheme.
- Requests: `get_wearable_gifts` (the filtered `getReceivedGifts`) and `set_upgraded_gift_colors`. New purposes `PaymentsPurpose::GetWearableGifts` and `SetUpgradedGiftColors`; the answer fills `UsersState::wearable_gifts`.
- Driver: `load_wearable_gifts`, `set_upgraded_gift_colors` (Premium only), and a thumbnail download for the gift stickers.
- `PeerColorDraft.gift_colors_id`: a gift change counts as a name change. Apply sends `setUpgradedGiftColors` when a gift is chosen, `setAccentColor` otherwise. Picking a swatch drops the gift. While a gift is worn the icon picker hides, as in tdesktop.
- UI: a "Wear a collectible" row of gift tiles on the Name tab; the preview takes the gift's accent.
- Entry points: "Your color" in Appearance settings (it opens the box on the Name tab), plus the deep links. `SettingsTarget::PeerColor(tab)` covers the `profile-color` and `your-color` subsections of the appearance and edit-profile sections; the `name` path part picks the Name tab, everything else the Profile tab. `add-icons`, `use-gift` and `reset` highlights are not reproduced.

## Left out
- Wearing a gift as the emoji status on the Profile tab (tdesktop's profile "wearable"): a separate emoji-status flow.
- Highlighting a control from the deep link subsection.
- Buying a gift from the picker (tdesktop's resale button).

## How verified
- gate.sh: GATE OK. New tests: gift colors parsing, request JSON, apply outcomes with gifts, driver (wearable list filtered to colored gifts, worn id read from `updateUser`, request body), and deep link routing for each subsection form plus the non-matching cases.
- A throwaway demo capture (not committed) showed the gift row with the worn gift selected.
- Not verified: a live account.
