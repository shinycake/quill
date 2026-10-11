# Boosts overview, level table and unrestrict-by-boosts

## What tdesktop does

- `info/channel_statistics/boosts/`: the boosts page opens with a level progress bar and the rows Level, Existing boosts, Premium subscribers (or members) and Boosts to level up, then the boosts list and boost link. The list and link already existed in Quill (see `codex-admin-links-boosts.md`).
- `ui/boxes/boost_box.cpp` `AddFeaturesList`: a table of "Level N Unlocks:" with what each level adds (stories per day, colors, emoji status, wallpapers, autotranslation, switching off ads, and so on), worded for channels or groups.
- `boxes/peers/edit_peer_permissions_box.cpp` `AddBoostsUnrestrictSlider`: supergroups get "Do not restrict boosters", a toggle plus a 1 to 5 slider, saved with `channels.setBoostsToUnblockRestrictions`.
- "Get Boosts via Gifts" opens the giveaway flow. Not part of this change: Quill has no giveaway creation yet.

## What changed

- Requests: `getChatBoostFeatures` and `setSupergroupUnrestrictBoostCount` (names checked in `schema/td_api.tl`, lines 14305 and 15571) in `src/telegram/requests/boosts_usernames.rs`.
- Envelope (`src/telegram/envelope/boost_features.rs`): `chatBoostStatus` is now parsed in full (`ParsedBoostStatus`), `chatBoostFeatures` into per-level feature records. `level_fraction`, `boosts_to_next_level` and `ParsedBoostFeatures::table` hold the progress math and the per-level unlock lines.
- State: `GroupsState` keeps the full status, the level table per chat and the unrestrict value being saved. The value is applied when TDLib answers `ok` and dropped on an error (`src/state/session_boost_settings.rs`), so a refusal never leaves a wrong number on screen and the hotspot rollback enum stays untouched.
- Driver: `fetch_chat_boost_features` and `set_unrestrict_boost_count`, which refuses values outside 0 to 8 and viewers without the right to restrict members.
- UI (`src/ui/boosts_overview.rs`): progress bar and stats, the unlock table, and in supergroups an Off / 1 to 5 button row. It sits at the top of the existing Boosts page in the group settings dialog.

TDLib accepts 0 to 8; tdesktop offers 1 to 5, and so does the UI.

## Verified

- Unit tests: status and feature parsing, level progress math, the unlock table for channels and groups, request JSON, the ok/error path of the unrestrict value, the permission gate, and the driver round trip. `gate.sh` prints GATE OK.
- Not verified: no live account run and no screenshot of the new section. The setting was not changed in any real chat.
