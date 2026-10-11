# Channel and group appearance

## What tdesktop does
`EditPeerColorBox` (`boxes/peers/edit_peer_color_box.cpp`) is opened from the
"Appearance" button of the channel edit box (`canEditEmoji`: owner or admin
with the change-info right). Channels get name color plus reply icon and
profile color plus profile icon; supergroups get only the profile part. Both
get an emoji status. Nothing needs Telegram Premium; each part needs a boost
level. Colors cost `PeerColors::requiredLevelFor` (the chat's own default
color is free, an unlisted color costs 1); icons and status cost
`channel_bg_icon_level_min`, `channel_profile_bg_icon_level_min` /
`group_profile_bg_icon_level_min` and `channel_emoji_status_level_min` /
`group_emoji_status_level_min`. Apply asks for the chat's current level
(`CheckBoostLevel`) and, when it is too low, sends nothing and shows the boost
box. The status picker uses the chat default statuses.

## What changed
- `src/chat_appearance.rs`: pure rules. Draft, per-part required levels (from
  the chat's `getChatBoostFeatures` table, tdesktop defaults when it is
  missing, palette minimums for colors), and `apply_outcome`: unchanged,
  level unknown, needs boosts, or the plan of changed parts.
- `setChatAccentColor`, `setChatProfileAccentColor`, `setChatEmojiStatus`,
  `getDefaultChatEmojiStatuses` (`telegram/requests_chat_appearance.rs`) and the
  driver in `connect/chat_appearance.rs`, which refuses chats the viewer may
  not edit and refetches the boost level on open.
- State: the chat's profile color, profile icon and emoji status
  (`chat_looks`, from `chat` and `updateChatAccentColors` /
  `updateChatEmojiStatus`); palettes keep `min_*_chat_boost_level`.
- `src/ui/chat_appearance_dialog.rs`: "Appearance" row in the group settings
  dialog and the box with Profile and Name tabs, swatches labelled with the
  level they need, icon grids, status grid, "Level N required" notes and a
  "Get boosts" button that opens the chat's Boosts page.

Not done: the chat wallpaper and group emoji pack rows of the same box, and
timed statuses for chats (tdesktop's channel panel has none either).

## How verified
Unit tests for the gating (levels, unchanged, unknown, too few, only changed
parts, supergroup), request JSON per target, parsing, reducer updates and the
driver (rights gate, one request per part, level refetch). The gate passes.
A temporary demo capture of the box (not committed) showed the Name tab with
level labels and the grids. No real chat's appearance was changed.
