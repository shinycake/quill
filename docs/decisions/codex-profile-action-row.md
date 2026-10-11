# Profile action row

## What tdesktop does

`info_profile_top_bar.cpp` / `info_profile_actions.cpp` show a row of round
icon buttons with labels under the profile cover: Message, Mute/Unmute, Call,
Video, Gift, and a More menu (Join/Leave, Discuss and Live stream for
channels). Call and Video are hidden for bots and yourself; Gift is hidden for
yourself and bots.

## What changed

- New `src/ui/profile_panels/action_row.rs`. `user_profile_actions` is a pure
  function from a few facts (self, bot, contact, open chat is this user's
  chat, calls allowed) to the ordered list of tiles. The row renderer wires
  each tile to the existing flow.
- Order is now Message, Mute, Call, Video, Gift, Secret chat, Add contact,
  Edit profile, More. The tile block was moved out of `user_info_panel.rs`.
- New tiles: Gift (opens the existing gift picker for the open private chat,
  `NavigationAction::Gift`) and More (the chat header's menu button, so it is
  the same menu). Both only show when the open chat is this user's chat, since
  both act on the open chat; from a group member's profile they are hidden.
- The row wraps instead of overflowing.

## Not done

- Group and channel profiles keep their current layout (no Join/Leave/Discuss
  tiles yet).
- "Call disabled if the user forbids calls" needs `userFullInfo.can_be_called`
  which Quill does not parse; Call stays visible and fails with the existing
  error.

## Verification

Unit tests for tile selection per peer kind (contact in own chat, member
profile, bot, stranger, self); gate.sh.
