# Profile action row and More menu

## What tdesktop does

- `Info::Profile::TopBar::setupActions` puts a row of icon buttons under a
  profile's name. A user gets Message. A chat the viewer has not joined
  gets Join. Everyone but yourself gets Mute. Then come up to three of
  Call, Discuss, Manage, Video Chat or Live Stream, Gift, Report and
  Leave, and finally More.
- A left click on Mute toggles mute forever. A right click opens
  `MuteMenu::FillMuteMenu`: Select tone, sound on or off, mute for 1 hour,
  8 hours or 2 days, Mute for..., then Mute forever or Unmute.
- More opens `Filler::fillProfileActions` (`window_peer_menu.cpp`). It
  holds Auto-Delete, add or share or edit contact, add members, Send Gift,
  statistics, story archive, manage, personal channel, view discussion,
  direct messages, export, Disable/Enable Sharing, folders, block, report,
  leave and delete contact.
- `addToggleNoForwards` shows the Disable Sharing box the first three
  times (`kDisableSharingBoxMaxShowsCount`). Premium users get the toggle.
  Everyone else gets the Premium preview on `NoForwards`. Turning sharing
  back on needs no box.
- `DetailsFiller::setupInfo` shows a public chat's link as "t.me/name"
  over "Link". When the chat has more usernames, the label reads "also
  @b, @c" instead. A click copies the link. Right click offers Copy Link
  and Share Link. A location-based group shows its address next.
- The profile's status line shows "N members, M online".

## What changed

- `quill::profile_actions` decides which buttons and More entries appear
  from plain facts, and has unit tests. The UI turns those facts into kit
  `Button`s, a `DropdownMenu` and a `ContextMenu`.
  - The buttons share the width in a grid.
  - More, Mute and their menus use the kit components.
- The Disable Sharing box is a kit `Dialog`. The show count lives in chat
  prefs (`disable_sharing_box_shown`). Users who are not Premium land on
  the Premium dialog's "Protect private chat content" page.
- `toggleChatHasProtectedContent` is now allowed in private chats with
  people (not bots), applied at once and rolled back on an error.
- `createPrivateChat` now runs for any person, so a profile opened without
  a chat can still show Message, Mute and Disable Sharing.
- `supergroupFullInfo.location` and `can_send_gift` are parsed into
  `SupergroupProfileFacts`.
- Group and channel profiles get the Link row (with the "also" usernames,
  copy, share and QR) and the Location row. Members online now shows for
  supergroups and basic groups.
- Five strings that the old row used are gone from `people.strings`.

## Deviations

- These More entries are left out: add bot to group, send money (TON),
  support info, ban from channel and topic entries.
- Quill keeps Video Call and Secret Chat in More. Your own profile keeps
  its Edit profile button.
- tdesktop can ask the other person to turn sharing back on. Quill skips
  that request box, because TDLib does not say who turned protection on.
- Story Archive shows only for the owner.
- The "also" usernames are plain text, not links.
- The Disable Sharing show count is per app, not per account.

## How it was verified

- Unit tests:
  - `profile_actions_tests`: which buttons and More entries appear.
  - The `group_profile` parser.
  - `connect::tests::profile_actions`: sharing toggle and rollback, bots
    refused, `createPrivateChat` sent once, location and gift flag stored.
  - `usernames_subtext`.
- GPUI tests (`profile_panels::action_row_tests`):
  - Mute toggles on click.
  - Right click lists the durations, and "Mute for 1 hour" sets
    `mute_for`.
  - More lists the contact entries and opens the Disable Sharing box.
  - The group shows Link, QR, Location and online counts.
  - The channel gets Discuss and Gift.
- `/verify` captures were read back for the `ready-profile-user`, `-group`
  and `-channel` demos in light and dark. They also cover the More menu
  (user and group), the Mute menu and the Disable Sharing box.
- Not verified: real TDLib responses, a Premium account toggling sharing,
  and the Premium preview landing on the right page with a live feature
  list.
