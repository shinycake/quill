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

## The list under the details (review follow-up)

tdesktop still draws the action list under a profile's details next to the
top bar. `InnerWidget::setupContent` adds `SetupActions` unconditionally
(`info_profile_inner_widget.cpp` lines 378-383). What that list holds:

- Users (`ActionsFiller::fillUserActions`, `info_profile_actions.cpp`
  lines 3204-3223): Share this contact, Edit contact, Delete contact, a
  bot's Report, then Block.
- Broadcast channels (`fillChannelActions`, lines 3225-3234): Join, Leave
  channel, and Report when the viewer is not the owner.
- Supergroups and basic groups: nothing, because `fill()` returns `nullptr`
  for them (lines 3236-3257).

So Quill keeps the user rows in tdesktop's order and the channel rows. It
removes these:

- **"Leave group" row.** A group now leaves from the Leave button and More.
- **Set, Suggest and Reset Profile Photo rows.** tdesktop has them in
  `EditContactBox` (`edit_contact_box.cpp` around line 615), so Quill now
  shows them in the Edit contact box, and in the Add contact box for
  someone who is not a contact.

The empty gap between Description and Members on a group had a cause in
the layout. The invite-links, join-requests, administrators, event-log
and boost sections returned an empty `div` when they had nothing to show.
Each empty `div` still took the body's `gap_3`. They now return `None`.

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
- `the_contact_photo_rows_live_in_the_edit_contact_box` and
  `only_a_channel_keeps_a_leave_row` cover the follow-up.
- `/verify` captures were read back for the `ready-profile-user`, `-group`
  and `-channel` demos in light and dark. They also cover the More menu
  (user and group), the Mute menu and the Disable Sharing box.
- Not verified: real TDLib responses, a Premium account toggling sharing,
  and the Premium preview landing on the right page with a live feature
  list.
