# Main menu: emoji status, My Stories, My Groups and Channels

## What tdesktop does

`window/window_main_menu.cpp` lists My Profile, New Group, New Channel,
Contacts, Calls, Saved Messages, Settings and the Night Mode switch. The
header has a "Set Emoji Status" / "Change Emoji Status" link
(`SetStatusLabel`) that opens the status panel, or the Premium preview for
accounts without Premium. In this version, right-click on New Group / New
Channel (`AddMyChannelsBox`) opens a box listing the groups or channels the
account created. A status chosen in the panel can be given a duration
(`info_profile_emoji_status_panel.cpp`, `PickUntilBox`).

## What Quill had

My Profile, Contacts, Calls, Saved Messages, Accounts, Night Mode and the
archive entries were already in the menu. The status request, the picker
panel and a duration row existed in the Emoji Packs and Status dialog, but
nothing in the menu led to them.

## What changed

- `src/main_menu.rs`: label rule, duration list, expiration math and the
  "created by me" filter, with unit tests. `change_emoji_status` in the
  driver now uses the same expiration helper (the request JSON with
  `expiration_date` was already tested in `requests_emoji.rs`).
- `src/ui/main_menu_extras.rs` (new): menu actions and the My Groups /
  My Channels dialog (opens the chat on click).
- Menu: My Stories (the account's story page), Set/Change Emoji Status,
  My Groups, My Channels.
- Status panel: right-click a status for For 1 hour / 2 hours / 8 hours /
  2 days / Other... ("Other..." waits for the hours field, then applies).

## Limits

- The My Groups and Channels lists only include chats whose own membership
  is already known to Quill (it is recorded when a chat is opened or its
  rights are fetched), so a chat never opened may be missing.
- The menu is a flat dropdown, so these live as plain entries rather than
  a right-click on New Group / New Channel.

## Verification

Unit tests for the helpers; gate.sh. The dropdown and dialog were not
captured (no demo shows the menu), and the real account's status was never
changed.
